// Runs the Tauri CLI from this folder for the app in ../src-tauri, on every system:
// `npm run tauri dev`, `npm run tauri build`, `npm run tauri -- build --bundles deb`.
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

process.env.TAURI_APP_PATH ??= fileURLToPath(new URL('../../src-tauri', import.meta.url));
process.env.TAURI_FRONTEND_PATH ??= fileURLToPath(new URL('..', import.meta.url));
const run = spawnSync('tauri', process.argv.slice(2), { stdio: 'inherit', shell: process.platform === 'win32' });
process.exit(run.status ?? 1);
