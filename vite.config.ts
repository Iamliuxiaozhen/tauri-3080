import { defineConfig } from 'vite';

// The frontend is a plain multi-page static site. The launcher (`index.html`)
// and the settings window (`settings.html`) are separate entry points, so the
// settings window never needs to boot the whole launcher UI.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: '127.0.0.1',
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'es2022',
    sourcemap: process.env.TAURI_ENV_DEBUG === 'true',
    minify: process.env.TAURI_ENV_DEBUG === 'true' ? false : 'oxc',
    rollupOptions: {
      input: {
        main: 'index.html',
        settings: 'settings.html',
      },
    },
  },
});
