import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { initTheme } from './lib/theme.svelte';

initTheme();

// The web view's own context menu (reload, inspect) has no place in the app; text fields keep it.
if (!import.meta.env.DEV) {
  document.addEventListener('contextmenu', (e) => {
    if (!(e.target as HTMLElement | null)?.closest('input, textarea')) e.preventDefault();
  });
}

const target = document.getElementById('app');
if (!target) throw new Error('no #app element');

export default mount(App, { target });
