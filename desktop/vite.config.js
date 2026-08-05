import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    cors: {
      origin: /^https?:\/\/(?:127\.0\.0\.1|localhost)(?::\d+)?$/,
    },
    hmr: {
      host: '127.0.0.1',
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  optimizeDeps: {
    entries: ['index.html'],
  },
  build: {
    target: ['es2021', 'chrome100', 'safari13'],
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
  resolve: {
    alias: {
      '$lib': path.resolve('./src/lib'),
    },
  },
});
