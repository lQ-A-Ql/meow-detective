import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * A literal `t('some.key')` that does not exist in the locale files renders the key itself,
 * which is how a raw key once reached the V2 page. Dynamic keys (`t(\`a.${b}\`)`) are skipped
 * because they cannot be checked statically.
 */
const srcRoot = path.resolve(process.cwd(), 'src');
const i18nRoot = path.join(srcRoot, 'i18n');

function sourceFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === 'i18n' ? [] : sourceFiles(full);
    return /\.tsx?$/.test(entry.name) ? [full] : [];
  });
}

function literalKeys(source: string): string[] {
  const keys: string[] = [];
  for (const match of source.matchAll(/\bt\(\s*'([^'\\]+)'/g)) keys.push(match[1]);
  for (const match of source.matchAll(/\bt\(\s*"([^"\\]+)"/g)) keys.push(match[1]);
  // A trailing dot means the literal is a prefix for a dynamic key, not a key of its own.
  return keys.filter((key) => !key.endsWith('.'));
}

function resolveKey(locale: Record<string, unknown>, key: string): boolean {
  return key.split('.').reduce<unknown>((node, part) => (
    node && typeof node === 'object' ? (node as Record<string, unknown>)[part] : undefined
  ), locale) !== undefined;
}

function missingFor(localeName: string, keys: Set<string>): string[] {
  const locale = JSON.parse(fs.readFileSync(path.join(i18nRoot, localeName + '.json'), 'utf8'));
  return [...keys].filter((key) => !resolveKey(locale, key)).sort();
}

describe('translation keys', () => {
  it('resolves every literal t() key in both locales', () => {
    const keys = new Set(sourceFiles(srcRoot).flatMap((file) => literalKeys(fs.readFileSync(file, 'utf8'))));

    expect({ missingZh: missingFor('zh-CN', keys), missingEn: missingFor('en', keys) })
      .toEqual({ missingZh: [], missingEn: [] });
  });
});
