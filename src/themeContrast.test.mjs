import { expect, it } from "vitest";
import { readFileSync } from "node:fs";
const css = readFileSync("src/index.css", "utf8");

function luminance(hex, brightness = 1) {
  const rgb = [1, 3, 5].map(offset => Math.min(1, parseInt(hex.slice(offset, offset + 2), 16) / 255 * brightness));
  return rgb.map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4)
    .reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
}

function contrast(a, b, brightness = 1) {
  const values = [luminance(a), luminance(b, brightness)].sort((x, y) => x - y);
  return (values[1] + 0.05) / (values[0] + 0.05);
}

it.each([":root", ':root[data-theme="light"]'])("keeps default accent roles readable in %s", selector => {
  const body = css.slice(css.indexOf(`${selector} {`)).split("}")[0];
  const tokens = Object.fromEntries([...body.matchAll(/--([\w-]+):\s*(#[\da-f]{6});/gi)].map(match => [match[1], match[2]]));
  for (const surface of ["panel-bg", "panel-bg-inactive", "header-bg", "accent-dim"]) {
    expect(contrast(tokens["accent-text"], tokens[surface]), `accent text on ${surface}`).toBeGreaterThanOrEqual(4.5);
  }
  for (const brightness of [1, 1.1]) {
    expect(contrast("#ffffff", tokens["accent-surface"], brightness), `white text at brightness ${brightness}`).toBeGreaterThanOrEqual(4.5);
  }
  for (const surface of ["panel-bg", "panel-bg-inactive", "header-bg"]) {
    expect(contrast(tokens.accent, tokens[surface]), `indicator on ${surface}`).toBeGreaterThanOrEqual(3);
  }
});
