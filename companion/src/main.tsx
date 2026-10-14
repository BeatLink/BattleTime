// The window's entry point, which Vite builds into dist/ for Tauri to serve.
//
// Opened in a plain browser (npm run dev) there is no window.neotime, so the mock watch stands in for Bluetooth.

import { render } from 'preact';

import { App } from '@/app';

import './styles.css';

render(<App />, document.getElementById('app')!);
