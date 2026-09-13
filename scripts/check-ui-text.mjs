// Find literal presentation text in JSX. Translation resources own UI wording;
// exceptions must identify an exact file/text pair and explain why it is data.
import ts from 'typescript';
import { readFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const namedAttributes = new Set(['aria-label', 'aria-description', 'title', 'alt', 'placeholder', 'label', 'description']);
const words = /\p{L}/u;
const tidy = text => text.replace(/\s+/g, ' ').trim();

export function inspect(file, source, translationKeys = new Set()) {
  const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const findings = [];
  function report(node, text, kind) {
    text = tidy(text);
    if (!words.test(text)) return;
    findings.push({file, line:tree.getLineAndCharacterOfPosition(node.getStart(tree)).line + 1, kind, text});
  }
  function expression(node, kind) {
    if (!node) return;
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) report(node, node.text, kind);
    else if (ts.isConditionalExpression(node)) { expression(node.whenTrue, kind); expression(node.whenFalse, kind); }
    else if (ts.isBinaryExpression(node)) {
      const operator = node.operatorToken.kind;
      if (operator === ts.SyntaxKind.AmpersandAmpersandToken) expression(node.right, kind);
      else if ([ts.SyntaxKind.BarBarToken, ts.SyntaxKind.QuestionQuestionToken, ts.SyntaxKind.PlusToken].includes(operator)) {
        expression(node.left, kind); expression(node.right, kind);
      }
    }
    else if (ts.isParenthesizedExpression(node)) expression(node.expression, kind);
    else if (ts.isTemplateExpression(node)) {
      report(node.head, node.head.text, kind);
      node.templateSpans.forEach(span => report(span.literal, span.literal.text, kind));
    }
    // Calls (including t/fillVars), identifiers and file data are resolved by
    // their owning module. Never treat translation keys as displayed literals.
  }
  function walk(node) {
    if (ts.isPropertyAssignment(node)) {
      const name = ts.isIdentifier(node.name) || ts.isStringLiteral(node.name) ? node.name.text : '';
      if (namedAttributes.has(name)) {
        const start = findings.length;
        expression(node.initializer, name);
        // Configuration may hold typed resource keys instead of rendered text.
        // Recognize only exact keys present in the resource files, never a
        // punctuation pattern. Direct JSX text is still checked without this.
        const entries = findings.splice(start);
        findings.push(...entries.filter(entry => !translationKeys.has(entry.text)));
      }
    }
    if (ts.isJsxText(node)) report(node, node.text, 'text');
    if (ts.isJsxAttribute(node) && namedAttributes.has(node.name.getText(tree))) {
      const value = node.initializer;
      if (value && ts.isStringLiteral(value)) report(value, value.text, node.name.getText(tree));
      else if (value && ts.isJsxExpression(value)) expression(value.expression, node.name.getText(tree));
    }
    if (ts.isJsxExpression(node) && (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))) expression(node.expression, 'text');
    ts.forEachChild(node, walk);
  }
  walk(tree);
  return findings;
}

function main() {
  const files = execFileSync('rg', ['--files', 'src', '--glob', '*.tsx', '--glob', '!*.test.tsx'], {encoding:'utf8'}).trim().split('\n');
  const exceptions = existsSync('scripts/ui-text-exceptions.json') ? JSON.parse(readFileSync('scripts/ui-text-exceptions.json', 'utf8')) : [];
  if (exceptions.some(entry => !entry.file || !entry.text || !entry.reason?.trim())) throw new Error('Every UI-text exception requires an exact file, text and reason.');
  const translationKeys = new Set();
  const resources = execFileSync('rg', ['--files', 'src/i18n/namespaces', '--glob', '*.ts'], {encoding:'utf8'}).trim().split('\n');
  for (const file of resources) {
    const tree = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
    function collect(node) {
      if (ts.isPropertyAssignment(node) && ts.isStringLiteral(node.name)) translationKeys.add(node.name.text);
      ts.forEachChild(node, collect);
    }
    collect(tree);
  }
  const findings = files.flatMap(file => inspect(file, readFileSync(file, 'utf8'), translationKeys));
  const remaining = findings.filter(f => !exceptions.some(e => e.file === f.file && e.text === f.text));
  const stale = exceptions.filter(e => !findings.some(f => e.file === f.file && e.text === f.text));
  if (process.argv.includes('--report')) console.log(JSON.stringify({remaining,stale}, null, 2));
  else {
    for (const f of remaining) console.error(`${f.file}:${f.line}: ${f.kind}: ${JSON.stringify(f.text)}`);
    for (const e of stale) console.error(`Unused UI-text exception: ${e.file}: ${e.text}`);
    console.log(`check-ui-text: ${files.length} components, ${remaining.length} untranslated literals, ${stale.length} unused exceptions.`);
    if (remaining.length || stale.length) process.exitCode = 1;
  }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
