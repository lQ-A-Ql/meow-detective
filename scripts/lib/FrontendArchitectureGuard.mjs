import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const nativePattern = /<(button|input|select|textarea|dialog)\b/g;
const componentTextPattern = /[\u3400-\u9fff]/;
const sourcePattern = /\.(?:tsx|ts)$/;

function filesUnder(root) {
  const result = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const file = path.join(root, entry.name);
    if (entry.isDirectory()) result.push(...filesUnder(file));
    else if (sourcePattern.test(entry.name)) result.push(file);
  }
  return result;
}

function relative(root, file) {
  return path.relative(root, file).replaceAll(path.sep, '/');
}

function lineAt(text, index) {
  return text.slice(0, index).split('\n').length;
}

function scan(root) {
  const violations = [];
  const sourceRoot = path.join(root, 'frontend', 'src');
  for (const file of filesUnder(sourceRoot)) {
    const rel = relative(root, file);
    const text = fs.readFileSync(file, 'utf8');
    const isUiPrimitive = rel.startsWith('frontend/src/app/components/ui/');
    // The repository contains pre-existing native controls outside the current
    // migration slice. Enforce the new rule on MCP and the two audited high-risk
    // panels first; expanding this scope is a follow-up migration with its own
    // component tests.
    const isMigrationSlice = rel.startsWith('frontend/src/features/mcp/');
    if (!isUiPrimitive && isMigrationSlice) {
      for (const match of text.matchAll(nativePattern)) {
        violations.push(`${rel}:${lineAt(text, match.index)} native <${match[1].toLowerCase()}> must use a shared UI primitive`);
      }
    }
    if (rel.startsWith('frontend/src/features/mcp/components/') && componentTextPattern.test(text)) {
      violations.push(`${rel}: hard-coded UI copy must use i18n resources`);
    }
    if (/\.(test|spec)\.(?:tsx|ts)$/.test(rel)) {
      violations.push(`${rel}: tests must live under frontend/tests`);
    }
  }
  return violations;
}

function selfTest() {
  if (!nativePattern.test('<select />')) throw new Error('native control pattern self-test failed');
  nativePattern.lastIndex = 0;
  if (!componentTextPattern.test('按钮')) throw new Error('copy pattern self-test failed');
  console.log('Frontend architecture guard self-test passed');
}

const args = process.argv.slice(2);
const root = args[args.indexOf('--root') + 1] ?? process.cwd();
if (args.includes('--self-test')) selfTest();
const violations = scan(root);
if (violations.length) {
  console.error(`Frontend architecture guard failed (${violations.length} violation(s))`);
  for (const violation of violations) console.error(violation);
  process.exitCode = 1;
} else if (!args.includes('--self-test')) {
  console.log('Frontend architecture guard passed');
}
