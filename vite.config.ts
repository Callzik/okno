import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Две страницы: index.html — настройки, live.html — живая сцена.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    rollupOptions: {
      input: { index: "index.html", live: "live.html" },
    },
  },
});
