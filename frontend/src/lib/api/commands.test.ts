import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { COMMANDS } from './commands';

function flattenCommands(obj: Record<string, Record<string, string>>): string[] {
  return Object.values(obj).flatMap((group) => Object.values(group));
}

function parseBackendHandlerCommands(): Set<string> {
  // Tests run from frontend/, backend lib.rs is at ../apps/desktop/src-tauri/src/lib.rs
  const libRs = path.resolve(process.cwd(), '..', 'apps', 'desktop', 'src-tauri', 'src', 'lib.rs');
  const text = fs.readFileSync(libRs, 'utf-8');
  const marker = 'tauri::generate_handler![';
  const start = text.indexOf(marker);
  expect(start).toBeGreaterThan(-1);
  const end = text.indexOf(']', start + marker.length);
  expect(end).toBeGreaterThan(start);
  const block = text.slice(start + marker.length, end);
  const cleaned = block
    .split('\n')
    .map((line) => line.replace(/\/\/.*$/, ''))
    .join(',');
  const names = new Set<string>();
  for (const token of cleaned.split(',')) {
    const t = token.trim();
    if (/^[a-z][a-z0-9_]*$/.test(t)) {
      names.add(t);
    }
  }
  return names;
}

describe('command registry contract', () => {
  it('has unique, non-empty snake_case command names', () => {
    const names = flattenCommands(COMMANDS);
    expect(names.length).toBeGreaterThan(0);
    expect(new Set(names).size).toBe(names.length);
    for (const name of names) {
      expect(name).toMatch(/^[a-z][a-z0-9_]*$/);
    }
  });

  it('every frontend command is registered in the Rust invoke handler', () => {
    const backend = parseBackendHandlerCommands();
    const missing = flattenCommands(COMMANDS).filter((name) => !backend.has(name));
    expect(missing).toEqual([]);
  });
});
