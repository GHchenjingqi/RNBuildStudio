import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

// Tauri 前端：固定 1420 端口（tauri.conf.json 的 devUrl 指向这里）。
// 不用 Vite 默认的 5173——那个端口在开发机上常被别的项目占用，
// 一旦撞上就会「exe/dev 窗口加载到别人的页面」。strictPort 保证被占用时直接报错，
// 而不是静默换端口让 devUrl 指空。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'es2021',
  },
});
