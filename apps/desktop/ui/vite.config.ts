import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync } from 'node:fs';

const cargo = readFileSync(new URL('../../../Cargo.toml', import.meta.url), 'utf8');
const version = cargo.match(/^version = "([^"]+)"/m)?.[1] ?? '0.0.0';

// The same build runs inside the Tauri window and in a plain browser, where a mock backend
// stands in for the Rust side (README screenshots, a possible web demo).
export default defineConfig({
  plugins: [svelte()],
  define: { __APP_VERSION__: JSON.stringify(version) },
  base: './',
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true, chunkSizeWarningLimit: 1024 },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
