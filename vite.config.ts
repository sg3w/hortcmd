import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { readFileSync } from "fs";

// Version aus der package.json — der Über-Dialog (TICKET-040) zeigt die
// tatsächliche Version und darf sie nicht fest verdrahten.
const pkg = JSON.parse(
  readFileSync(resolve(__dirname, "package.json"), "utf8"),
) as { version: string };

// Tauri erwartet einen festen Port und lauscht im Dev-Modus lokal.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: ["es2021", "chrome100", "safari13"],
    minify: process.env.TAURI_DEBUG ? false : "esbuild",
    sourcemap: !!process.env.TAURI_DEBUG,
  },
  // Tests run in jsdom: the preview sanitisation (TICKET-030) works on
  // real nodes, and the component tests need a document at all.
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // One setup for every file: a working localStorage, and a reset of
    // the DOM, timers, mocks, Tauri harness and Zustand stores between
    // tests (TICKET-033).
    setupFiles: ["src/test/setup.ts"],
    restoreMocks: true,
    clearMocks: true,
    unstubEnvs: true,
    unstubGlobals: true,
  },
});
