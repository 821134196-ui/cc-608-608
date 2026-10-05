import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// 开发：5173 端口，/api 与 /uploads 反代到 Axum(8080)
// 生产：vite build 产物由 Axum 直接托管（FRONTEND_DIST=frontend/dist）
export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    host: '127.0.0.1',
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/uploads': 'http://127.0.0.1:8080',
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
