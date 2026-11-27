// Screenshots for the README: serves the built front end (mock backend) and lets headless
// Chrome photograph it in a few states. Run with `npm run screenshots`; writes docs/images/.
import { createServer } from 'node:http';
import { readFile, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../dist/', import.meta.url));
const out = fileURLToPath(new URL('../../../../docs/images/', import.meta.url));
const chrome = process.env.CHROME ?? 'google-chrome';
const only = process.argv[2];

const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.json': 'application/json' };

const server = createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, 'http://x').pathname)).replace(/^(\.\.[/\\])+/, '');
  const file = join(root, path === '/' ? 'index.html' : path);
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end();
  }
});

await new Promise((r) => server.listen(0, '127.0.0.1', r));
const port = server.address().port;
await mkdir(out, { recursive: true });

const shots = [
  ['desktop-light.png', '?demo&select&lang=en&theme=light'],
  ['desktop-dark.png', '?demo&select&lang=en&theme=dark'],
  ['desktop-preview.png', '?demo&preview=0.62&lang=en&theme=light'],
  ['desktop-send.png', '?demo&send=change&lang=en&theme=light'],
  ['desktop-print.png', '?demo&print&lang=en&theme=light'],
];

for (const [name, query] of shots) {
  if (only && !name.includes(only)) continue;
  const url = `http://127.0.0.1:${port}/index.html${query}`;
  // A profile of its own, so that no running Chrome takes the job over; asynchronous, because
  // this process also serves the pages Chrome asks for.
  const profile = await mkdtemp(join(tmpdir(), 'bladepilot-shot-'));
  try {
    await promisify(execFile)(
      chrome,
      [
        `--user-data-dir=${profile}`,
        '--headless',
        '--disable-gpu',
        '--hide-scrollbars',
        '--force-device-scale-factor=1',
        '--window-size=1440,900',
        '--virtual-time-budget=4000',
        `--screenshot=${join(out, name)}`,
        url,
      ],
      { timeout: 60000 },
    );
  } finally {
    await rm(profile, { recursive: true, force: true });
  }
  // 256 colours look the same here and take a third of the space; skipped without ImageMagick.
  const file = join(out, name);
  await promisify(execFile)('magick', [file, '-strip', '-dither', 'None', '-colors', '256', '-define', 'png:compression-level=9', `PNG8:${file}`]).catch(() => {});
  console.log(`wrote docs/images/${name}`);
}

server.close();
