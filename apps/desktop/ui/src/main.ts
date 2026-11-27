import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { initTheme } from './lib/theme.svelte';

initTheme();

const target = document.getElementById('app');
if (!target) throw new Error('no #app element');

export default mount(App, { target });
