/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // TC-Theme-Tokens (siehe index.css :root)
        bg: "var(--bg)",
        panel: "var(--panel-bg)",
        "panel-inactive": "var(--panel-bg-inactive)",
        header: "var(--header-bg)",
        edge: "var(--border)",
        text: "var(--text)",
        dim: "var(--text-dim)",
        accent: "var(--accent)",
        "accent-dim": "var(--accent-dim)",
        selection: "var(--selection)",
        "selection-text": "var(--selection-text)",
        dir: "var(--dir-color)",
        file: "var(--file-color)",
        exec: "var(--exec-color)",
        "row-alt": "var(--row-alt)",
        cursor: "var(--cursor-active)",
      },
      fontFamily: {
        ui: "var(--font-ui)",
        mono: "var(--font-mono)",
      },
    },
  },
  plugins: [],
};
