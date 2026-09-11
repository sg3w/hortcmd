// ============================================================
// Render helpers for the file preview: view modes per format,
// Markdown rendering (sanitized), syntax highlighting, and a
// lean CSV parser.
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

// Register only a curated set of languages (lean bundle).
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

/** View modes of the preview. */
export type PreviewMode =
  | "rendered"
  | "highlight"
  | "table"
  | "raw"
  | "hex"
  | "text";

/** Extension of a file name (lowercase, without the dot). */
function extOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** Extension → highlight.js language (null = no known language). */
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

/** Available modes + default mode for a file. */
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

// ============================================================
// Markdown sanitization (TICKET-030).
//
// A previewed file is untrusted input that ends up as real DOM inside
// the window holding the IPC surface, so the sanitizer is configured
// explicitly rather than left on its defaults: only the elements
// Markdown actually produces survive, only `http(s)` and in-document
// fragments are accepted as URLs, and every anchor is neutralised for
// navigation. The CSP is the second line — this is the first.
// ============================================================

/** Exactly what Markdown renders — no `style`, `form`, `iframe`, `svg`. */
const ALLOWED_TAGS = [
  "p", "br", "hr", "div", "span",
  "h1", "h2", "h3", "h4", "h5", "h6",
  "strong", "b", "em", "i", "s", "del", "ins", "sub", "sup", "mark", "small",
  "blockquote", "pre", "code", "kbd", "samp", "var",
  "ul", "ol", "li",
  "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption",
  "a", "img",
  "dl", "dt", "dd",
];

/** No `style`, no `on*`, no `target`, no `srcset`. */
const ALLOWED_ATTR = [
  "href", "title", "alt", "src", "width", "height",
  "colspan", "rowspan", "start", "type", "class", "id", "align",
];

/**
 * Absolute `http(s)` links and in-document fragments only.
 *
 * A *relative* href is deliberately excluded too: it would resolve
 * against the app document and — being same-origin — pass the
 * navigation policy, replacing the running app. Stripping it here means
 * the click interception below is a convenience, not the only guard.
 */
const ALLOWED_URI_REGEXP = /^(?:https?:\/\/|#)/i;

/** Anchors stay visible but carry nothing a click could exploit. */
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.nodeName !== "A") return;
  node.removeAttribute("target");
  node.setAttribute("rel", "noopener noreferrer");
});

/** Markdown → sanitized HTML for `dangerouslySetInnerHTML`. */
export function renderMarkdown(md: string): string {
  const html = marked.parse(md) as string;
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOWED_URI_REGEXP,
    // Keep the text of a dropped element, drop the element itself.
    KEEP_CONTENT: true,
    // No `<template>`/shadow-DOM escape hatches, no full document.
    WHOLE_DOCUMENT: false,
    FORBID_CONTENTS: ["script", "style", "iframe", "object", "embed"],
  });
}

/**
 * Is this a link a preview may hand to the system browser? Mirrors the
 * backend's own check (`is_safe_external_url`) so an unsupported scheme
 * produces no IPC call at all; the backend still validates, because the
 * command must not trust its caller either.
 */
export function isExternalLink(href: string): boolean {
  return /^https?:\/\/[^\s/?#]+/i.test(href);
}

/** Source code → highlighted HTML (falls back to plain text).
 *  `hljs.highlight` escapes the source it wraps in token spans, and the
 *  fallback escapes by hand — nothing from the file reaches the DOM as
 *  markup either way. */
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
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Extension of a name (public, for language/mode detection). */
export function extNameOf(name: string): string {
  return extOf(name);
}

/**
 * Lean CSV/TSV parser: handles quotes ("") and
 * line breaks inside fields. Separator = tab for .tsv, otherwise comma.
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
  // Append the last field/row (if there is no trailing line break).
  if (field.length > 0 || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}
