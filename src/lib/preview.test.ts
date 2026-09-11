// ============================================================
// Security tests for the preview's rich-content rendering
// (TICKET-030).
//
// The subject is hostile *file content*: a previewed Markdown or source
// file is untrusted input that ends up as real DOM inside the window
// that holds the IPC surface. Every case below asserts that nothing
// executable, navigable or IPC-reaching survives.
// ============================================================

import { describe, expect, it } from "vitest";
import { highlightCode, isExternalLink, renderMarkdown } from "./preview";

/** Rendered Markdown as a DOM tree, so structure can be queried. */
function render(md: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = renderMarkdown(md);
  return host;
}

describe("renderMarkdown", () => {
  it("drops scripts and keeps nothing executable", () => {
    const html = renderMarkdown("<script>alert(1)</script>\n\nHallo");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("alert(1)");
    expect(html).toContain("Hallo");
  });

  it("strips inline event handlers", () => {
    const host = render('<p onclick="alert(1)" onerror="alert(2)">x</p>');
    const p = host.querySelector("p");
    expect(p).not.toBeNull();
    expect(p!.getAttribute("onclick")).toBeNull();
    expect(p!.getAttribute("onerror")).toBeNull();
    expect(host.innerHTML).not.toContain("alert");
  });

  it("removes javascript: links", () => {
    const host = render("[klick](javascript:alert(1))");
    expect(host.querySelector("a[href]")).toBeNull();
    expect(host.innerHTML.toLowerCase()).not.toContain("javascript:");
  });

  it("removes data: links and data: images", () => {
    const host = render(
      "[a](data:text/html,<script>alert(1)</script>)\n\n" +
        "![b](data:image/svg+xml,<svg onload=alert(1)>)",
    );
    // No element carries a data: URL. Such a URL may survive as inert
    // literal text when Markdown does not parse it as a link at all —
    // that is text, not a navigation or loading surface.
    expect(host.querySelector("a[href]")).toBeNull();
    expect(host.querySelector("img[src]")).toBeNull();
    expect(host.querySelector("svg")).toBeNull();
    for (const el of host.querySelectorAll("*")) {
      for (const attr of el.attributes) {
        expect(attr.value.toLowerCase()).not.toContain("data:");
      }
    }
  });

  it("removes relative links, which would replace the app document", () => {
    // Same-origin, so the navigation policy would allow it — the
    // sanitizer has to be the one that stops it.
    const host = render("[hier](index.html) und [dort](/etc/passwd)");
    expect(host.querySelectorAll("a[href]")).toHaveLength(0);
  });

  it("keeps http(s) links and neutralises them for navigation", () => {
    const host = render("[example](https://example.com/a?b=1)");
    const a = host.querySelector("a");
    expect(a).not.toBeNull();
    expect(a!.getAttribute("href")).toBe("https://example.com/a?b=1");
    expect(a!.getAttribute("rel")).toBe("noopener noreferrer");
    expect(a!.getAttribute("target")).toBeNull();
  });

  it("keeps in-document fragments", () => {
    const host = render("[oben](#heading)");
    expect(host.querySelector("a")!.getAttribute("href")).toBe("#heading");
  });

  it("drops iframes, objects and embeds", () => {
    const host = render(
      '<iframe src="https://evil.test"></iframe>' +
        '<object data="x.swf"></object><embed src="x.swf">',
    );
    expect(host.querySelector("iframe")).toBeNull();
    expect(host.querySelector("object")).toBeNull();
    expect(host.querySelector("embed")).toBeNull();
  });

  it("drops svg payloads", () => {
    const host = render('<svg><script>alert(1)</script></svg>');
    expect(host.querySelector("svg")).toBeNull();
    expect(host.innerHTML).not.toContain("alert(1)");
  });

  it("drops style elements and style attributes", () => {
    const host = render('<style>body{display:none}</style><p style="x">t</p>');
    expect(host.querySelector("style")).toBeNull();
    expect(host.querySelector("p")!.getAttribute("style")).toBeNull();
  });

  it("drops forms and inputs", () => {
    const host = render('<form action="https://evil.test"><input name="p"></form>');
    expect(host.querySelector("form")).toBeNull();
    expect(host.querySelector("input")).toBeNull();
  });

  it("survives encoded and obfuscated schemes", () => {
    for (const href of [
      "java\tscript:alert(1)",
      "JaVaScRiPt:alert(1)",
      "&#106;avascript:alert(1)",
      " javascript:alert(1)",
      "vbscript:msgbox(1)",
    ]) {
      const host = render(`[x](${href})`);
      const a = host.querySelector("a[href]");
      const value = a?.getAttribute("href")?.toLowerCase() ?? "";
      expect(value).not.toContain("javascript");
      expect(value).not.toContain("vbscript");
    }
  });

  it("survives nested and malformed HTML", () => {
    const host = render(
      '<div><p><script>alert(1)</p></div><img src=x onerror="alert(2)"><<>>',
    );
    expect(host.querySelector("script")).toBeNull();
    expect(host.innerHTML).not.toContain("onerror");
    expect(host.innerHTML).not.toContain("alert(");
  });

  it("renders ordinary markdown intact", () => {
    const host = render("# Titel\n\n- eins\n- zwei\n\n`code` und **fett**");
    expect(host.querySelector("h1")!.textContent).toBe("Titel");
    expect(host.querySelectorAll("li")).toHaveLength(2);
    expect(host.querySelector("code")!.textContent).toBe("code");
    expect(host.querySelector("strong")!.textContent).toBe("fett");
  });
});

describe("highlightCode", () => {
  it("escapes closing tags and handler attributes in source", () => {
    const src = '</code></pre><img src=x onerror="alert(1)">';
    const html = highlightCode(src, "js");
    expect(html).not.toContain("<img");
    expect(html).not.toContain("onerror=");
    expect(html).toContain("&lt;");
  });

  it("escapes source of an unknown language too", () => {
    const html = highlightCode('<script>alert(1)</script>', "unknown-ext");
    expect(html).not.toContain("<script");
    expect(html).toContain("&lt;script&gt;");
  });

  it("does not execute what it renders", () => {
    const host = document.createElement("div");
    host.innerHTML = highlightCode('<img src=x onerror="alert(1)">', "html");
    expect(host.querySelector("img")).toBeNull();
  });
});

describe("isExternalLink", () => {
  it("accepts absolute http(s) URLs with a host", () => {
    expect(isExternalLink("https://example.com")).toBe(true);
    expect(isExternalLink("http://example.com/a?b=1#c")).toBe(true);
    expect(isExternalLink("HTTPS://Example.com")).toBe(true);
  });

  it("refuses everything else, so no IPC call is made", () => {
    for (const href of [
      "",
      "#anchor",
      "index.html",
      "/etc/passwd",
      "//example.com",
      "file:///etc/passwd",
      "javascript:alert(1)",
      "data:text/html,x",
      "sftp://host/path",
      "https://",
      " https://example.com",
    ]) {
      expect(isExternalLink(href), href).toBe(false);
    }
  });
});
