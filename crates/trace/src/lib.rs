//! Outlines from pixels.
//!
//! A [`Field`] holds how much each pixel belongs to the shape (0 = background, 1 = shape): an
//! image's alpha channel, its darkness, or a design drawn with anti-aliasing. [`Mask`] is the
//! field cut at a threshold. From there:
//!
//! - [`Mask::despeckle`] removes specks and closes pinholes below a size,
//! - [`Mask::fill_holes`] fills everything enclosed, for an outer outline only,
//! - [`trace`] follows every edge of the shape (holes included) with sub-pixel precision,
//! - [`contour`] follows the line at a given distance around the shape, for cutting around a
//!   printed sticker.
//!
//! Lines come out as closed polylines in pixel units, `(0, 0)` at the top-left corner of the
//! top-left pixel, simplified to a tolerance.

/// How much each pixel belongs to the shape, 0 to 1, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

impl Field {
    pub fn new(width: usize, height: usize, values: Vec<f32>) -> Field {
        assert_eq!(values.len(), width * height, "one value per pixel");
        Field { width, height, values }
    }

    pub fn from_fn(width: usize, height: usize, f: impl Fn(usize, usize) -> f32) -> Field {
        let mut values = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                values.push(f(x, y));
            }
        }
        Field { width, height, values }
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.values[y * self.width + x]
    }

    /// The pixels at or above `threshold`.
    pub fn mask(&self, threshold: f32) -> Mask {
        Mask { width: self.width, height: self.height, data: self.values.iter().map(|&v| v >= threshold).collect() }
    }

    /// A smaller field, every `factor` × `factor` block averaged.
    pub fn shrink(&self, factor: usize) -> Field {
        if factor <= 1 {
            return self.clone();
        }
        let (w, h) = (self.width.div_ceil(factor), self.height.div_ceil(factor));
        let mut values = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n) = (0.0f32, 0u32);
                for yy in y * factor..((y + 1) * factor).min(self.height) {
                    for xx in x * factor..((x + 1) * factor).min(self.width) {
                        sum += self.get(xx, yy);
                        n += 1;
                    }
                }
                values[y * w + x] = if n > 0 { sum / n as f32 } else { 0.0 };
            }
        }
        Field { width: w, height: h, values }
    }
}

/// Which pixels belong to the shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    data: Vec<bool>,
}

impl Mask {
    pub fn new(width: usize, height: usize) -> Mask {
        Mask { width, height, data: vec![false; width * height] }
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.height && self.data[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, value: bool) {
        if x < self.width && y < self.height {
            self.data[y * self.width + x] = value;
        }
    }

    /// Number of pixels set.
    pub fn count(&self) -> usize {
        self.data.iter().filter(|&&v| v).count()
    }

    /// Connected regions of pixels whose value is `value`: their pixel indices, and whether they
    /// touch the border. Shape pixels connect in 8 directions, background pixels in 4, so that a
    /// diagonal line of shape pixels closes a hole.
    fn regions(&self, value: bool) -> Vec<(Vec<usize>, bool)> {
        let (w, h) = (self.width, self.height);
        let mut seen = vec![false; w * h];
        let mut out = Vec::new();
        let mut stack = Vec::new();
        for start in 0..w * h {
            if seen[start] || self.data[start] != value {
                continue;
            }
            let mut pixels = Vec::new();
            let mut border = false;
            seen[start] = true;
            stack.push(start);
            while let Some(i) = stack.pop() {
                pixels.push(i);
                let (x, y) = (i % w, i / w);
                if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                    border = true;
                }
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    if !value && dx != 0 && dy != 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if !seen[j] && self.data[j] == value {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            out.push((pixels, border));
        }
        out
    }

    /// Removes specks of fewer than `min_pixels` pixels and closes holes of fewer than
    /// `min_pixels` pixels inside the shape.
    pub fn despeckle(&mut self, min_pixels: usize) {
        if min_pixels == 0 {
            return;
        }
        for (pixels, _) in self.regions(true) {
            if pixels.len() < min_pixels {
                for i in pixels {
                    self.data[i] = false;
                }
            }
        }
        for (pixels, border) in self.regions(false) {
            if !border && pixels.len() < min_pixels {
                for i in pixels {
                    self.data[i] = true;
                }
            }
        }
    }

    /// Fills everything the shape encloses, so that only outer outlines remain.
    pub fn fill_holes(&mut self) {
        for (pixels, border) in self.regions(false) {
            if !border {
                for i in pixels {
                    self.data[i] = true;
                }
            }
        }
    }

    /// The mask as a field: 1 for shape, 0 for background.
    pub fn to_field(&self) -> Field {
        Field { width: self.width, height: self.height, values: self.data.iter().map(|&v| if v { 1.0 } else { 0.0 }).collect() }
    }
}

/// A closed polyline in pixel units.
pub type Outline = Vec<(f64, f64)>;

/// Every edge of the shape, holes included: the line where `field` crosses `threshold`,
/// limited to the pixels `mask` keeps (after despeckling, say). Simplified to `tolerance` pixels.
pub fn trace(field: &Field, mask: &Mask, threshold: f32, tolerance: f64) -> Vec<Outline> {
    // Inside the mask the field keeps its anti-aliased edge; outside it counts as background,
    // so a removed speck leaves no line and a closed pinhole none either.
    let lo = threshold - 0.5;
    let hi = threshold + 0.5;
    let values = field
        .values
        .iter()
        .zip(&mask.data)
        .map(|(&v, &inside)| {
            if inside {
                v.max(threshold)
            } else if v >= threshold {
                lo
            } else {
                v.min(hi)
            }
        })
        .collect();
    let field = Field { width: field.width, height: field.height, values };
    iso_lines(&field, threshold).into_iter().map(|line| simplify(&line, tolerance)).filter(|l| l.len() >= 4).collect()
}

/// The outline at `distance` pixels around the shape (its holes filled), for cutting around a
/// sticker. With `distance` 0 it follows the shape's outer edge.
pub fn contour(mask: &Mask, distance: f64, tolerance: f64) -> Vec<Outline> {
    let mut filled = mask.clone();
    filled.fill_holes();
    if filled.count() == 0 {
        return Vec::new();
    }
    // Room for the grown outline around the mask.
    let pad = distance.ceil() as usize + 2;
    let (w, h) = (filled.width + 2 * pad, filled.height + 2 * pad);
    let mut grown = Mask::new(w, h);
    for y in 0..filled.height {
        for x in 0..filled.width {
            if filled.get(x, y) {
                grown.set(x + pad, y + pad, true);
            }
        }
    }
    let lines = if distance <= 0.0 {
        iso_lines(&grown.to_field(), 0.5)
    } else {
        // Inside-ness falls off with the distance from the shape; the line sits where it is
        // exactly `distance` away.
        let dist = distance_to(&grown);
        let field = Field { width: w, height: h, values: dist.iter().map(|&d| (distance as f32 + 0.5) - d).collect() };
        iso_lines(&field, 0.5)
    };
    lines
        .into_iter()
        .map(|line| simplify(&line.into_iter().map(|(x, y)| (x - pad as f64, y - pad as f64)).collect::<Vec<_>>(), tolerance))
        .filter(|l| l.len() >= 4)
        .collect()
}

/// Euclidean distance from every pixel centre to the nearest set pixel centre (0 on the shape).
pub fn distance_to(mask: &Mask) -> Vec<f32> {
    const INF: f64 = 1e20;
    let (w, h) = (mask.width, mask.height);
    let mut grid: Vec<f64> = mask.data.iter().map(|&v| if v { 0.0 } else { INF }).collect();
    let mut column = vec![0.0; h];
    let mut out = vec![0.0; h.max(w)];
    for x in 0..w {
        for y in 0..h {
            column[y] = grid[y * w + x];
        }
        squared_distance_1d(&column, &mut out[..h]);
        for y in 0..h {
            grid[y * w + x] = out[y];
        }
    }
    let mut row = vec![0.0; w];
    for y in 0..h {
        row.copy_from_slice(&grid[y * w..(y + 1) * w]);
        squared_distance_1d(&row, &mut out[..w]);
        grid[y * w..(y + 1) * w].copy_from_slice(&out[..w]);
    }
    grid.into_iter().map(|d| d.sqrt() as f32).collect()
}

/// Felzenszwalb and Huttenlocher's lower envelope of parabolas: for every index, the smallest
/// `f[q] + (index - q)²`.
fn squared_distance_1d(f: &[f64], out: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64);
            if s > z[k] {
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f64::INFINITY;
                break;
            }
            // z[0] is minus infinity, so this never steps below the first parabola.
            k -= 1;
        }
    }
    let mut k = 0usize;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let d = q as f64 - p as f64;
        *o = d * d + f[p];
    }
}

/// Marching squares: the closed lines where `field` crosses `level`, sample points at pixel
/// centres. The field counts as below `level` outside its border, so every line closes.
fn iso_lines(field: &Field, level: f32) -> Vec<Outline> {
    let (w, h) = (field.width + 2, field.height + 2);
    let outside = level - 1.0;
    let at = |x: usize, y: usize| -> f32 { if x == 0 || y == 0 || x + 1 == w || y + 1 == h { outside } else { field.get(x - 1, y - 1) } };
    // Edge ids: horizontal edge from (x, y) to (x + 1, y) is 2·(y·w + x), vertical edge from
    // (x, y) to (x, y + 1) is 2·(y·w + x) + 1.
    let horizontal = |x: usize, y: usize| 2 * (y * w + x);
    let vertical = |x: usize, y: usize| 2 * (y * w + x) + 1;
    let point = |edge: usize| -> (f64, f64) {
        let cell = edge / 2;
        let (x, y) = (cell % w, cell / w);
        let (x2, y2) = if edge.is_multiple_of(2) { (x + 1, y) } else { (x, y + 1) };
        let (a, b) = (at(x, y), at(x2, y2));
        let t = if (b - a).abs() < 1e-12 { 0.5 } else { f64::from((level - a) / (b - a)).clamp(0.0, 1.0) };
        // Sample (x, y) of the padded grid is the centre of pixel (x - 1, y - 1).
        let px = x as f64 + t * (x2 as f64 - x as f64) - 0.5;
        let py = y as f64 + t * (y2 as f64 - y as f64) - 0.5;
        (px, py)
    };
    // Each segment runs between two edges; every crossed edge belongs to exactly two segments.
    let mut next: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    let mut segments: Vec<(usize, usize)> = Vec::new();
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let (tl, tr, br, bl) = (at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1));
            let case = usize::from(tl >= level) << 3 | usize::from(tr >= level) << 2 | usize::from(br >= level) << 1 | usize::from(bl >= level);
            let (top, right, bottom, left) = (horizontal(x, y), vertical(x + 1, y), horizontal(x, y + 1), vertical(x, y));
            let centre_inside = (tl + tr + br + bl) / 4.0 >= level;
            let pairs: &[(usize, usize)] = match case {
                0 | 15 => &[],
                1 | 14 => &[(left, bottom)],
                2 | 13 => &[(bottom, right)],
                3 | 12 => &[(left, right)],
                4 | 11 => &[(top, right)],
                6 | 9 => &[(top, bottom)],
                7 | 8 => &[(left, top)],
                5 => {
                    if centre_inside {
                        &[(left, top), (bottom, right)]
                    } else {
                        &[(left, bottom), (top, right)]
                    }
                }
                10 => {
                    if centre_inside {
                        &[(left, bottom), (top, right)]
                    } else {
                        &[(left, top), (bottom, right)]
                    }
                }
                _ => &[],
            };
            for &(a, b) in pairs {
                let i = segments.len();
                segments.push((a, b));
                next.entry(a).or_default().push(i);
                next.entry(b).or_default().push(i);
            }
        }
    }
    let mut used = vec![false; segments.len()];
    let mut lines = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (first, mut edge) = segments[start];
        let mut line = vec![point(first), point(edge)];
        while edge != first {
            let Some(&seg) = next.get(&edge).and_then(|list| list.iter().find(|&&s| !used[s])) else {
                break;
            };
            used[seg] = true;
            let (a, b) = segments[seg];
            edge = if a == edge { b } else { a };
            line.push(point(edge));
        }
        if line.len() >= 4 {
            if line.first() != line.last()
                && let Some(&p) = line.first()
            {
                line.push(p);
            }
            lines.push(line);
        }
    }
    lines
}

/// Ramer–Douglas–Peucker on a closed polyline (first point repeated at the end).
pub fn simplify(line: &[(f64, f64)], tolerance: f64) -> Outline {
    if line.len() < 4 || tolerance <= 0.0 {
        return line.to_vec();
    }
    // Split the loop at the point farthest from the start, so both halves are open lines.
    let start = line[0];
    let far = (1..line.len() - 1).max_by(|&a, &b| dist2(line[a], start).total_cmp(&dist2(line[b], start))).unwrap_or(line.len() / 2);
    let mut out = rdp(&line[..=far], tolerance);
    out.pop();
    out.extend(rdp(&line[far..], tolerance));
    out
}

fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

fn rdp(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    let mut worst = (0usize, 0.0f64);
    for (i, &p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = if len < 1e-12 { (p.0 - a.0).hypot(p.1 - a.1) } else { ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len };
        if d > worst.1 {
            worst = (i, d);
        }
    }
    if worst.1 <= tolerance {
        return vec![a, b];
    }
    let mut left = rdp(&points[..=worst.0], tolerance);
    left.pop();
    left.extend(rdp(&points[worst.0..], tolerance));
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(size: usize, cx: f64, cy: f64, r: f64) -> Field {
        Field::from_fn(size, size, |x, y| {
            // Anti-aliased edge: coverage falls off over one pixel.
            let d = ((x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy)) - r;
            (0.5 - d).clamp(0.0, 1.0) as f32
        })
    }

    fn area(line: &Outline) -> f64 {
        line.windows(2).map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1).sum::<f64>().abs() / 2.0
    }

    #[test]
    fn a_disc_traces_to_a_circle() {
        let field = disc(60, 30.0, 30.0, 20.0);
        let mask = field.mask(0.5);
        let lines = trace(&field, &mask, 0.5, 0.1);
        assert_eq!(lines.len(), 1);
        for &(x, y) in &lines[0] {
            let r = (x - 30.0).hypot(y - 30.0);
            assert!((r - 20.0).abs() < 0.35, "r = {r}");
        }
        assert!((area(&lines[0]) - std::f64::consts::PI * 400.0).abs() < 15.0, "{}", area(&lines[0]));
    }

    #[test]
    fn a_ring_traces_to_two_lines_and_its_contour_to_one() {
        let field = Field::from_fn(80, 80, |x, y| {
            let r = (x as f64 + 0.5 - 40.0).hypot(y as f64 + 0.5 - 40.0);
            if (15.0..=30.0).contains(&r) { 1.0 } else { 0.0 }
        });
        let mask = field.mask(0.5);
        assert_eq!(trace(&field, &mask, 0.5, 0.2).len(), 2, "outer edge and hole");
        let around = contour(&mask, 5.0, 0.2);
        assert_eq!(around.len(), 1, "the hole is filled");
        for &(x, y) in &around[0] {
            let r = (x - 40.0).hypot(y - 40.0);
            assert!((r - 35.0).abs() < 0.8, "r = {r}");
        }
    }

    #[test]
    fn specks_go_and_pinholes_close() {
        let mut mask = Field::from_fn(50, 50, |x, y| {
            let block = (10..40).contains(&x) && (10..40).contains(&y);
            let pinhole = (24..26).contains(&x) && (24..26).contains(&y);
            let speck = (2..4).contains(&x) && (2..4).contains(&y);
            if (block && !pinhole) || speck { 1.0 } else { 0.0 }
        })
        .mask(0.5);
        assert_eq!(mask.count(), 900 - 4 + 4);
        mask.despeckle(10);
        assert_eq!(mask.count(), 900, "speck gone, pinhole closed");
        let field = mask.to_field();
        assert_eq!(trace(&field, &mask, 0.5, 0.2).len(), 1);
    }

    #[test]
    fn distances_are_euclidean() {
        let mut mask = Mask::new(21, 21);
        mask.set(10, 10, true);
        let d = distance_to(&mask);
        assert_eq!(d[10 * 21 + 10], 0.0);
        assert!((d[10 * 21 + 13] - 3.0).abs() < 1e-6);
        assert!((d[14 * 21 + 13] - 5.0).abs() < 1e-6, "3-4-5");
        assert!((d[0] - (200.0f32).sqrt()).abs() < 1e-4);
    }

    #[test]
    fn a_square_contour_has_round_corners_at_the_distance() {
        let mask = Field::from_fn(40, 40, |x, y| if (10..30).contains(&x) && (10..30).contains(&y) { 1.0 } else { 0.0 }).mask(0.5);
        let around = contour(&mask, 4.0, 0.1);
        assert_eq!(around.len(), 1);
        // The square spans pixel centres 10.5..29.5, its edge 10..30; the contour runs 4 px out
        // from the centres of the outermost pixels, half a pixel beyond the edge plus 3.5.
        let xs: Vec<f64> = around[0].iter().map(|p| p.0).collect();
        let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!((min - 6.0).abs() < 0.6 && (max - 34.0).abs() < 0.6, "{min} {max}");
        // A corner is rounded: no point near the square's diagonal corner at (6, 6).
        assert!(around[0].iter().all(|&(x, y)| (x - 6.0).hypot(y - 6.0) > 0.8));
    }

    #[test]
    fn an_empty_mask_has_no_outline() {
        let mask = Mask::new(10, 10);
        assert!(contour(&mask, 3.0, 0.1).is_empty());
        assert!(trace(&mask.to_field(), &mask, 0.5, 0.1).is_empty());
    }

    #[test]
    fn simplify_keeps_corners_and_drops_collinear_points() {
        let square: Outline = vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (0.0, 0.0)];
        let s = simplify(&square, 0.1);
        assert_eq!(s.len(), 5, "{s:?}");
        assert_eq!(s.first(), s.last());
    }
}
