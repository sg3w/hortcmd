// ============================================================
// Render-Helfer für die Datei-Vorschau: Anzeigemodi je Format,
// Markdown-Rendering (sanitisiert), Syntaxhighlighting und ein
// schlanker CSV-Parser.
// ============================================================

import { marked } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/core";
import xml from "highlight.js/lib/languages/xml";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import json from "highlight.js/lib/languages/json";
import yaml from "highlight.js/lib/languages/yaml";
import css from "highlight.js/lib/languages/css";
import markdown from "highlight.js/lib/languages/markdown";
import bash from "highlight.js/lib/languages/bash";
import rust from "highlight.js/lib/languages/rust";
import python from "highlight.js/lib/languages/python";
import sql from "highlight.js/lib/languages/sql";
import ini from "highlight.js/lib/languages/ini";

// Nur eine kuratierte Sprachmenge registrieren (schlankes Bundle).
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("json", json);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("css", css);
hljs.registerLanguage("markdown", markdown);
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("python", python);
hljs.registerLanguage("sql", sql);
hljs.registerLanguage("ini", ini);

marked.setOptions({ async: false, gfm: true, breaks: false });

/** Anzeigemodi der Vorschau. */
export type PreviewMode =
  | "rendered"
  | "highlight"
  | "table"
  | "raw"
  | "hex"
  | "text";

/** Endung eines Dateinamens (klein, ohne Punkt). */
function extOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** Endung → highlight.js-Sprache (null = keine bekannte Sprache). */
export function languageForExt(ext: string): string | null {
  const map: Record<string, string> = {
    html: "xml",
    htm: "xml",
    xml: "xml",
    svg: "xml",
    js: "javascript",
    mjs: "javascript",
    cjs: "javascript",
    jsx: "javascript",
    ts: "typescript",
    tsx: "typescript",
    json: "json",
    yaml: "yaml",
    yml: "yaml",
    css: "css",
    scss: "css",
    less: "css",
    md: "markdown",
    markdown: "markdown",
    sh: "bash",
    bash: "bash",
    zsh: "bash",
    rs: "rust",
    py: "python",
    sql: "sql",
    toml: "ini",
    ini: "ini",
    conf: "ini",
    cfg: "ini",
  };
  return map[ext] ?? null;
}

/** Verfügbare Modi + Standardmodus für eine Datei. */
export function viewModesFor(
  name: string,
  hasHex: boolean,
): { modes: PreviewMode[]; def: PreviewMode } {
  if (hasHex) return { modes: ["hex", "text"], def: "hex" };
  const ext = extOf(name);
  if (ext === "md" || ext === "markdown")
    return { modes: ["rendered", "raw"], def: "rendered" };
  if (ext === "csv" || ext === "tsv")
    return { modes: ["table", "raw"], def: "table" };
  if (languageForExt(ext)) return { modes: ["highlight", "raw"], def: "highlight" };
  return { modes: ["raw"], def: "raw" };
}

/** Markdown → sanitisiertes HTML. */
export function renderMarkdown(md: string): string {
  const html = marked.parse(md) as string;
  return DOMPurify.sanitize(html);
}

/** Quelltext → hervorgehobenes HTML (fällt auf reinen Text zurück). */
export function highlightCode(text: string, ext: string): string {
  const lang = languageForExt(ext);
  if (lang && hljs.getLanguage(lang)) {
    return hljs.highlight(text, { language: lang, ignoreIllegals: true }).value;
  }
  return escapeHtml(text);
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/** Endung eines Namens (öffentlich, für die Sprach-/Modusermittlung). */
export function extNameOf(name: string): string {
  return extOf(name);
}

/**
 * Schlanker CSV/TSV-Parser: berücksichtigt Anführungszeichen ("") und
 * Zeilenumbrüche innerhalb von Feldern. Trennzeichen = Tab bei .tsv, sonst Komma.
 */
export function parseCsv(text: string, name: string): string[][] {
  const delim = extOf(name) === "tsv" ? "\t" : ",";
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let inQuotes = false;

  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        field += c;
      }
      continue;
    }
    if (c === '"') {
      inQuotes = true;
    } else if (c === delim) {
      row.push(field);
      field = "";
    } else if (c === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else if (c !== "\r") {
      field += c;
    }
  }
  // Letztes Feld/Zeile anhängen (falls kein abschließender Umbruch).
  if (field.length > 0 || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}
