// ============================================================
// File-colour matching and priority (TICKET-033).
//
// Colour is how the list conveys Git state, hidden files and the user's
// own categories at a glance, so "which rule wins" is a visible
// contract: custom rules before built-ins, and within each group the
// declared priority order. The fallbacks matter too — a hand-edited
// config must still render rather than produce `undefined` in a CSS
// variable.
// ============================================================

import { describe, expect, it } from "vitest";
import {
  applyFileColors,
  customColorDefs,
  customColorId,
  fileColorCss,
  fileColorDefs,
  fileColorRef,
  fileColorValue,
  fileColorVar,
  normalizeColor,
  resolveFileColor,
  type CustomColorRule,
  type FileColorContext,
  type FileColorDef,
} from "./fileColors";

function ctx(over: Partial<FileColorContext> = {}): FileColorContext {
  return {
    name: "a.txt",
    is_dir: false,
    is_symlink: false,
    hidden: false,
    readonly: false,
    executable: false,
    ...over,
  } as FileColorContext;
}

function rule(over: Partial<CustomColorRule> = {}): CustomColorRule {
  return {
    id: "r1",
    name: "Logs",
    pattern: "*.log",
    dark: "#ff0000",
    light: "#880000",
    ...over,
  };
}

/** The active rule set, in the order `useFileColorRules` produces. */
function activeRules(custom: CustomColorRule[] = []): FileColorDef[] {
  return [...fileColorDefs(), ...customColorDefs(custom)]
    .filter((d) => d.kind === "rule")
    .sort((a, b) => a.priority - b.priority);
}

describe("custom rule matching", () => {
  it("matches its wildcard pattern", () => {
    const [def] = customColorDefs([rule()]);
    expect(def.match?.(ctx({ name: "server.log" }))).toBe(true);
    expect(def.match?.(ctx({ name: "server.txt" }))).toBe(false);
  });

  it("accepts several patterns in one rule", () => {
    const [def] = customColorDefs([rule({ pattern: "*.log *.tmp" })]);
    expect(def.match?.(ctx({ name: "a.log" }))).toBe(true);
    expect(def.match?.(ctx({ name: "a.tmp" }))).toBe(true);
    expect(def.match?.(ctx({ name: "a.md" }))).toBe(false);
  });

  it("treats an empty pattern as inactive rather than matching everything", () => {
    const [def] = customColorDefs([rule({ pattern: "   " })]);
    expect(def.match?.(ctx({ name: "anything" }))).toBe(false);
  });

  it("falls back to the pattern when the rule has no name", () => {
    const [def] = customColorDefs([rule({ name: "  " })]);
    expect(def.label).toBe("*.log");
  });

  it("namespaces its slot id", () => {
    expect(customColorDefs([rule()])[0].id).toBe(customColorId("r1"));
    expect(customColorId("r1")).toBe("custom.r1");
  });
});

describe("priority", () => {
  it("lets a custom rule win over a built-in one", () => {
    const hit = resolveFileColor(
      activeRules([rule({ pattern: "*.log" })]),
      ctx({ name: "a.log", hidden: true }),
    );
    expect(hit?.id).toBe("custom.r1");
  });

  it("keeps the user's own order among custom rules", () => {
    const rules = [
      rule({ id: "first", pattern: "*.log" }),
      rule({ id: "second", pattern: "*.log" }),
    ];
    expect(resolveFileColor(activeRules(rules), ctx({ name: "a.log" }))?.id).toBe(
      "custom.first",
    );
  });

  it("falls through to the built-ins when no custom rule matches", () => {
    const hit = resolveFileColor(
      activeRules([rule({ pattern: "*.log" })]),
      ctx({ name: ".profile", hidden: true }),
    );
    expect(hit?.id).not.toContain("custom.");
    expect(hit).toBeTruthy();
  });

  it("returns nothing for an entry no rule describes", () => {
    expect(resolveFileColor(activeRules(), ctx({ name: "plain.txt" }))).toBeUndefined();
  });

  it("sorts strictly ascending, so the winner is deterministic", () => {
    const priorities = activeRules([rule()]).map((d) => d.priority);
    expect([...priorities].sort((a, b) => a - b)).toEqual(priorities);
  });
});

describe("colour values", () => {
  const def = customColorDefs([rule()])[0];

  it("uses the stored override when it is a valid hex literal", () => {
    expect(fileColorValue(def, "dark", { [def.id]: { dark: "#00ff00" } })).toBe(
      "#00ff00",
    );
  });

  it("falls back to the rule's own colour for a broken override", () => {
    for (const broken of ["red", "#fff", "", "#gggggg"]) {
      expect(
        fileColorValue(def, "dark", { [def.id]: { dark: broken } }),
        broken,
      ).toBe("#ff0000");
    }
  });

  it("follows the theme", () => {
    expect(fileColorValue(def, "light", {})).toBe("#880000");
  });

  it("never yields undefined, even for a hand-edited default", () => {
    expect(normalizeColor(undefined)).toMatch(/^#[0-9a-f]{6}$/i);
    expect(normalizeColor("nonsense")).toMatch(/^#[0-9a-f]{6}$/i);
    expect(normalizeColor("#123456")).toBe("#123456");
  });

  it("renders an alpha slot as rgba", () => {
    const faded: FileColorDef = { ...def, alpha: 0.5 };
    expect(fileColorCss(faded, "dark", {})).toBe("rgba(255, 0, 0, 0.5)");
  });

  it("names its CSS variable from the slot id", () => {
    expect(fileColorVar("git.modified")).toBe("--fc-git-modified");
    expect(fileColorRef("git.modified")).toBe("var(--fc-git-modified)");
  });
});

describe("publishing to the document", () => {
  it("writes every slot and removes a rule that is gone again", () => {
    applyFileColors("dark", {}, [rule()]);
    const style = document.documentElement.style;
    expect(style.getPropertyValue(fileColorVar("custom.r1"))).toBe("#ff0000");

    applyFileColors("dark", {}, []);
    expect(style.getPropertyValue(fileColorVar("custom.r1"))).toBe("");
    // The built-ins stay published.
    const builtin = fileColorDefs()[0];
    expect(style.getPropertyValue(fileColorVar(builtin.id))).toBeTruthy();
  });
});

describe("Git meaning and theme defaults (061)", () => {
  it.each([
    ["added", "git.staged", "#4ADE80", "#166534"],
    ["modified", "git.modified", "#60A5FA", "#1D4ED8"],
    ["untracked", "git.untracked", "#F87171", "#B91C1C"],
  ])("publishes %s consistently with overrides and reset", (status, id, dark, light) => {
    const def = resolveFileColor(activeRules(), ctx({gitStatus: status}))!;
    expect(def.id).toBe(id);
    for (const [theme, color] of [["dark", dark], ["light", light]] as const) {
      applyFileColors(theme, {}, []);
      expect(fileColorValue(def, theme, {})).toBe(color);
      expect(document.documentElement.style.getPropertyValue(fileColorVar(id))).toBe(color);
      expect(fileColorValue(def, theme, {[id]: {[theme]: "#123456"}})).toBe("#123456");
      // Reset removes overrides; the same defaults also feed settings previews.
      expect(fileColorValue(def, theme, {})).toBe(color);
    }
    expect(def.labelKey).toBe(`color.git.${status}`);
  });
});
