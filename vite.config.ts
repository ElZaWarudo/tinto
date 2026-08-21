/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  test: {
    environment: "jsdom",
    fileParallelism: false,
    setupFiles: "./src/setupTests.ts",
    globals: false,
  },

  build: {
    // Shiki's oniguruma runtime is already loaded on demand, but its generated
    // WASM wrapper is ~622 kB before gzip. Keep the warning focused on chunks
    // that would represent a real regression in the initial application shell.
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      output: {
        // Keep the large, stable UI runtimes out of the application chunk.
        // Besides avoiding a monolithic entry bundle, this lets WebView reuse
        // vendor chunks when Tinto's own code changes between releases.
        manualChunks: {
          "vendor-dockview": ["dockview-react"],
          "vendor-markdown": ["react-markdown", "remark-gfm"],
          "vendor-shiki": ["shiki/core", "shiki/engine/oniguruma"],
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
