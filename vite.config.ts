import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react({ fastRefresh: false }), tailwindcss()],

  // Rutas relativas (Tauri asset protocol)
  base: "./",

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: false,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2021",
    minify: true,
    cssCodeSplit: false,
    // IIFE = script clásico (WebView2 de Tauri a veces no ejecuta type=module)
    modulePreload: false,
    rollupOptions: {
      output: {
        format: "iife",
        name: "AdcBible",
        inlineDynamicImports: true,
        entryFileNames: "assets/app.js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
}));
