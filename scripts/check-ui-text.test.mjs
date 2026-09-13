import { test } from 'node:test';
import assert from 'node:assert/strict';
import { inspect } from './check-ui-text.mjs';

test('finds rendered literals and both conditional accessible names', () => {
  const result = inspect('UI.tsx', '<button aria-label={open ? "Close" : "Open"} title={`Remove ${name}`}>Delete</button>');
  assert.deepEqual(result.map(f => f.text), ['Close', 'Open', 'Remove', 'Delete']);
});
test('does not confuse translation keys, CSS classes, or file data with wording', () => {
  assert.deepEqual(inspect('UI.tsx', '<button className="border red" aria-label={t("file.remove")}>{name}{t("file.delete")}</button>'), []);
});
test('finds literal fallback text inside rendered expressions', () => {
  assert.deepEqual(inspect('UI.tsx', '<span>{name || "Unnamed"}</span>').map(f => f.text), ['Unnamed']);
});

test('ignores comparison guards while checking the rendered branch', () => {
  assert.deepEqual(inspect('UI.tsx', '<div>{mode === "details" && <span>{t("view.details")}</span>}</div>'), []);
});

test('checks option labels and descriptions supplied through configuration objects', () => {
  const result = inspect('UI.tsx', 'const options = [{ value: "dark", label: "Dark" }, { value: "light", label: t("theme.light") }]; const props = { "aria-label": "Choose theme", description: active ? "Enabled" : "Disabled" };');
  assert.deepEqual(result.map(f => f.text), ['Dark', 'Choose theme', 'Enabled', 'Disabled']);
});

test('recognizes only exact resource keys in configuration, not rendered keys or unknown dotted wording', () => {
  const source = 'const options = [{label: "view.details"}, {label: "Unknown.word"}]; <span>view.details</span>';
  assert.deepEqual(inspect('UI.tsx', source, new Set(['view.details'])).map(f => f.text), ['Unknown.word', 'view.details']);
});
