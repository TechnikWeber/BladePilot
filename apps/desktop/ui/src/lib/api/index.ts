// Picks the backend: the Rust side inside the Tauri window, the mock in a plain browser.
import { mockBackend } from './mock';
import { inTauri, tauriBackend } from './tauri';
import type { Backend } from './types';

export const backend: Backend = inTauri() ? tauriBackend : mockBackend;
export * from './types';
