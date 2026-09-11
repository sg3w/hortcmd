import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { readFileSync } from "fs";

// Version aus der package.json — der Über-Dialog (TICKET-040) zeigt die
// tatsächliche Version und darf sie nicht fest verdrahten.
const pkg = JSON.parse(
  readFileSync(resolve(__dirname, "package.json"), "utf8"),
) as { version: string };

/**
 * Size budget for the eager entry chunk, in kB before compression
 * (TICKET-034). Raising it is a deliberate decision: see the "Frontend
 * loading boundaries" section of Concept.md for what belongs in the
 * eager chunk and what has to move behind a lazy boundary instead.
 * Enforced by `scripts/bundle-report.mjs` (`npm run check:bundle`).
 */
export const EAGER_BUDGET_KB = 320;

/**
 * Coarse backstop for *any* single chunk. Rollup's own warning cannot
 * tell an eager chunk from a lazy one, so it is set above the largest
 * deliberate lazy chunk (the terminal's xterm bundle, ~340 kB) — high
 * enough not to repeat a check the report makes precisely, low enough
 * to still notice a new dependency of that size appearing anywhere.
 */
const CHUNK_WARN_KB = 400;

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
    chunkSizeWarningLimit: CHUNK_WARN_KB,
    // The manifest is what `scripts/bundle-report.mjs` reads to tell an
    // eager chunk from a lazy one; index.html references the assets
    // directly, so it costs nothing else.
    manifest: true,
    rollupOptions: {
      output: {
        // Vendors change far less often than application code, so
        // separating them keeps their chunks cached across releases.
        // Deliberately only four coarse groups: finer splitting would
        // multiply requests on a local custom protocol for no gain.
        manualChunks: {
          "vendor-react": ["react", "react-dom", "react-dom/client"],
          "vendor-radix": [
            "@radix-ui/react-context-menu",
            "@radix-ui/react-dialog",
            "@radix-ui/react-dropdown-menu",
            "@radix-ui/react-menubar",
            "@radix-ui/react-select",
          ],
          "vendor-table": ["@tanstack/react-table", "@tanstack/react-virtual"],
          "vendor-icons": ["lucide-react"],
        },
      },
    },
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
