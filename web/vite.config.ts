import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import Icons from 'unplugin-icons/vite';

const apiPort = process.env.CTX_DEV_API_PORT || '8080';

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte(), Icons({ compiler: 'svelte' })],
  base: './',
  build: {
    // ctx-web embeds this via rust-embed #[folder = "dist"] (ADR-0005 Wave 4;
    // was ../internal/web/dist under the deleted Go implementation).
    outDir: '../crates/ctx-web/dist',
    emptyOutDir: true,
    sourcemap: false,
    target: 'es2022',
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: `http://127.0.0.1:${apiPort}`,
        changeOrigin: false,
      },
      '/raw': {
        target: `http://127.0.0.1:${apiPort}`,
        changeOrigin: false,
      },
    },
  },
});
