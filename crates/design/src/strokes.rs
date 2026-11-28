//! Digits drawn as single strokes, for numbers a pen writes (the alignment test's labels): each
//! digit is a few straight lines on a 2 × 4 grid, like a seven-segment display.

use crate::Polyline;

/// The strokes of one character on a grid 2 wide and 4 high, y down.
fn glyph(c: char) -> &'static [&'static [(f64, f64)]] {
    match c {
        '0' => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)]],
        '1' => &[&[(0.4, 0.8), (1.2, 0.0), (1.2, 4.0)]],
        '2' => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (0.0, 4.0), (2.0, 4.0)]],
        '3' => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0)], &[(0.0, 2.0), (2.0, 2.0)]],
        '4' => &[&[(0.0, 0.0), (0.0, 2.0), (2.0, 2.0)], &[(2.0, 0.0), (2.0, 4.0)]],
        '5' => &[&[(2.0, 0.0), (0.0, 0.0), (0.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]],
        '6' => &[&[(2.0, 0.0), (0.0, 0.0), (0.0, 4.0), (2.0, 4.0), (2.0, 2.0), (0.0, 2.0)]],
        '7' => &[&[(0.0, 0.0), (2.0, 0.0), (0.8, 4.0)]],
        '8' => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)], &[(0.0, 2.0), (2.0, 2.0)]],
        '9' => &[&[(2.0, 2.0), (0.0, 2.0), (0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0)]],
        '-' => &[&[(0.2, 2.0), (1.8, 2.0)]],
        '+' => &[&[(0.2, 2.0), (1.8, 2.0)], &[(1.0, 1.2), (1.0, 2.8)]],
        _ => &[],
    }
}

/// The width of `text` drawn `height_mm` high.
pub fn text_width(text: &str, height_mm: f64) -> f64 {
    let unit = height_mm / 4.0;
    let n = text.chars().count() as f64;
    if n == 0.0 { 0.0 } else { (n * 3.0 - 1.0) * unit }
}

/// `text` (digits, `-` and `+`) as strokes, its top-left corner at `at`, `height_mm` high.
pub fn digits(text: &str, at: (f64, f64), height_mm: f64) -> Vec<Polyline> {
    let unit = height_mm / 4.0;
    let mut out = Vec::new();
    for (i, c) in text.chars().enumerate() {
        let x0 = at.0 + i as f64 * 3.0 * unit;
        for stroke in glyph(c) {
            out.push(stroke.iter().map(|&(x, y)| (x0 + x * unit, at.1 + y * unit)).collect());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_as_high_as_asked_and_side_by_side() {
        let lines = digits("10", (5.0, 2.0), 2.0);
        let xs: Vec<f64> = lines.iter().flatten().map(|p| p.0).collect();
        let ys: Vec<f64> = lines.iter().flatten().map(|p| p.1).collect();
        let (x0, x1) = xs.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        let (y0, y1) = ys.iter().fold((f64::MAX, f64::MIN), |(a, b), &y| (a.min(y), b.max(y)));
        assert!((y0 - 2.0).abs() < 1e-9 && (y1 - 4.0).abs() < 1e-9);
        assert!(x0 >= 5.0 && x1 <= 5.0 + text_width("10", 2.0) + 1e-9);
        assert_eq!(digits("x", (0.0, 0.0), 2.0).len(), 0);
    }
}
