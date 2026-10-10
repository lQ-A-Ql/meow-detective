import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * The locale files are hand-edited next to the code, and JSON.parse silently hides duplicate
 * keys (last one wins). A duplicated object once shadowed a sibling key, so the UI rendered the
 * raw key name instead of its copy. These checks keep that class of bug out.
 */

const localeDir = path.resolve(process.cwd(), 'src', 'i18n');
const locales = ['zh-CN', 'en'] as const;

function readLocale(locale: string): string {
  return fs.readFileSync(path.join(localeDir, locale + '.json'), 'utf8');
}

/** Key paths that appear more than once inside the same JSON object. */
function duplicateKeys(text: string): string[] {
  const found: string[] = [];
  const stack: { keys: Set<string> }[] = [];
  const trail: string[] = [];
  let inString = false;
  let value = '';
  let escaped = false;

  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];

    if (inString) {
      if (escaped) {
        value += ch;
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === '"') {
        inString = false;
        let next = i + 1;
        while (next < text.length && /\s/.test(text[next])) next += 1;
        if (text[next] === ':' && stack.length > 0) {
          const scope = stack[stack.length - 1];
          if (scope.keys.has(value)) {
            found.push(trail.concat(value).join('.'));
          } else {
            scope.keys.add(value);
          }
          trail.push(value);
        }
      } else {
        value += ch;
      }
      continue;
    }

    if (ch === '"') {
      inString = true;
      value = '';
    } else if (ch === '{') {
      stack.push({ keys: new Set() });
    } else if (ch === '}') {
      stack.pop();
      trail.pop();
    }
  }

  return found;
}

function leafKeys(node: unknown, prefix = ''): string[] {
  if (node === null || typeof node !== 'object' || Array.isArray(node)) {
    return [prefix];
  }
  return Object.entries(node as Record<string, unknown>).flatMap(([key, value]) =>
    leafKeys(value, prefix ? prefix + '.' + key : key),
  );
}

describe('locale integrity', () => {
  it('has no duplicate keys inside any locale object', () => {
    for (const locale of locales) {
      expect({ locale, duplicates: duplicateKeys(readLocale(locale)) }).toEqual({ locale, duplicates: [] });
    }
  });

  it('keeps the zh-CN and en leaf key sets identical', () => {
    const sets = locales.map((locale) => new Set(leafKeys(JSON.parse(readLocale(locale)))));
    const onlyZh = [...sets[0]].filter((key) => !sets[1].has(key));
    const onlyEn = [...sets[1]].filter((key) => !sets[0].has(key));
    expect({ onlyZh, onlyEn }).toEqual({ onlyZh: [], onlyEn: [] });
  });

  it('keeps the navigation copy the activity rail relies on', () => {
    const zh = JSON.parse(readLocale('zh-CN'));
    expect(zh.navigation.appNavLabel).toBeTypeOf('string');
    expect(zh.navigation.workspaceNavLabel).toBeTypeOf('string');
    expect(zh.navigation.jobsRunning).toBeTypeOf('string');
    expect(Object.keys(zh.navigation.links).length).toBeGreaterThan(0);
  });
});
