import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

/**
 * Nothing verified the Tauri command *argument* contract: the api tests replace
 * apiClient.request entirely, so a payload key that no Rust command accepts passes every
 * test and only fails at runtime (there is no mock mode to catch it earlier).
 *
 * This test derives the accepted argument set from the Rust command signatures and checks
 * every statically readable apiClient.request(COMMANDS.x.Y, { ... }) payload against it.
 */

const repoRoot = path.resolve(process.cwd(), '..');
const commandsDir = path.join(repoRoot, 'apps', 'desktop', 'src-tauri', 'src', 'commands');
const apiDir = path.join(process.cwd(), 'src', 'lib', 'api');
const commandsFile = path.join(apiDir, 'commands.ts');

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

const toCamel = (value: string) => value.replace(/_([a-z0-9])/g, (_match, ch: string) => ch.toUpperCase());

/** Command name -> accepted JSON argument names, derived from each Rust command signature. */
function rustCommandArguments(): Map<string, Set<string>> {
  const map = new Map<string, Set<string>>();
  const signature = /pub\s+(?:async\s+)?fn\s+([a-z0-9_]+)\s*\(/g;
  for (const file of walk(commandsDir).filter((name) => name.endsWith('.rs'))) {
    const text = fs.readFileSync(file, 'utf8');
    let match: RegExpExecArray | null;
    while ((match = signature.exec(text))) {
      const open = match.index + match[0].length - 1;
      let depth = 0;
      let cursor = open;
      for (; cursor < text.length; cursor += 1) {
        const ch = text[cursor];
        if (ch === '(') depth += 1;
        else if (ch === ')') {
          depth -= 1;
          if (depth === 0) break;
        }
      }
      const keys = new Set<string>();
      for (const rawParam of text.slice(open + 1, cursor).split(',')) {
        const param = rawParam.trim();
        const separator = param.indexOf(':');
        if (separator < 0) continue;
        const paramName = param.slice(0, separator).trim().replace(/^(?:mut|ref)\s+/, '');
        const type = param.slice(separator + 1);
        if (!/^[a-z_][a-z0-9_]*$/.test(paramName)) continue;
        // Framework-injected handles are not part of the JSON payload.
        if (/State\s*<|AppHandle|WebviewWindow|Window\s*<|Request\s*<|tauri::ipc/.test(type)) continue;
        keys.add(toCamel(paramName));
      }
      map.set(match[1], keys);
    }
  }
  return map;
}

/** COMMANDS.<group>.<CONST> identifier -> wire command string (ambiguous constants dropped). */
function commandConstants(): Map<string, string | undefined> {
  const text = fs.readFileSync(commandsFile, 'utf8');
  const map = new Map<string, string | undefined>();
  const entry = /([A-Z0-9_]+)\s*:\s*'([a-z0-9_]+)'/g;
  let match: RegExpExecArray | null;
  while ((match = entry.exec(text))) {
    map.set(match[1], map.has(match[1]) ? undefined : match[2]);
  }
  return map;
}

/** Top-level keys of an inline object literal, or undefined when it cannot be read statically. */
function literalKeys(literal: string): string[] | undefined {
  const trimmed = literal.trim();
  if (!trimmed.startsWith('{')) return undefined;
  if (trimmed.includes('...')) return undefined;
  const keys: string[] = [];
  let depth = 0;
  let quote: string | undefined;
  let token = '';
  const flush = () => {
    if (depth !== 1) return;
    const value = token.trim();
    if (!value) return;
    const separator = value.indexOf(':');
    const key = (separator < 0 ? value : value.slice(0, separator)).trim().replace(/^['"]|['"]$/g, '');
    if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) keys.push(key);
  };
  for (let i = 0; i < trimmed.length; i += 1) {
    const ch = trimmed[i];
    if (quote) {
      if (ch === '\\') i += 1;
      else if (ch === quote) quote = undefined;
      continue;
    }
    if (ch === '"' || ch === "'" || ch === '\u0060') { quote = ch; continue; }
    if (ch === '{' || ch === '(' || ch === '[') { depth += 1; token = ''; continue; }
    if (ch === '}' || ch === ')' || ch === ']') { flush(); depth -= 1; token = ''; continue; }
    if (ch === ',' && depth === 1) { flush(); token = ''; continue; }
    if (depth === 1) token += ch;
  }
  return keys;
}

function callSites(): { file: string; constant?: string; keys: string[] }[] {
  const sites: { file: string; constant?: string; keys: string[] }[] = [];
  const call = /apiClient\.request(?:<[^>]*>)?\(\s*COMMANDS\.([A-Za-z0-9_]+)\.([A-Z0-9_]+)\s*,\s*/g;
  for (const file of walk(apiDir).filter((name) => name.endsWith('.ts'))) {
    const text = fs.readFileSync(file, 'utf8');
    let match: RegExpExecArray | null;
    while ((match = call.exec(text))) {
      const after = text.slice(match.index + match[0].length);
      let depth = 0;
      let cursor = 0;
      for (; cursor < after.length; cursor += 1) {
        const ch = after[cursor];
        if (ch === '{') depth += 1;
        else if (ch === '}') {
          depth -= 1;
          if (depth === 0) { cursor += 1; break; }
        } else if (depth === 0 && (ch === ';' || ch === '\n')) break;
      }
      sites.push({
        file: path.relative(repoRoot, file).replace(/\\/g, '/'),
        constant: match[2],
        keys: literalKeys(after.slice(0, cursor)) ?? [],
      });
    }
  }
  return sites;
}

describe('Tauri command argument contract', () => {
  const rust = rustCommandArguments();
  const constants = commandConstants();

  it('derives a useful number of Rust command signatures', () => {
    expect(rust.size).toBeGreaterThan(50);
  });

  it('every statically readable api payload only uses accepted argument names', () => {
    const constantsByCommand = new Map<string, string>();
    for (const [constant, command] of constants) {
      if (command) constantsByCommand.set(constant, command);
    }
    const sites = callSites().filter((site) => site.constant && site.keys.length > 0);
    expect(sites.length).toBeGreaterThan(15);

    const mismatches: string[] = [];
    let checked = 0;
    for (const site of sites) {
      const command = constantsByCommand.get(site.constant!);
      if (!command) continue;
      const accepted = rust.get(command);
      if (!accepted) continue;
      checked += 1;
      for (const key of site.keys) {
        if (!accepted.has(key)) {
          mismatches.push(site.file + ': ' + command + ' does not accept "' + key + '" (accepts: ' + [...accepted].sort().join(', ') + ')');
        }
      }
    }
    expect(checked).toBeGreaterThan(10);
    expect(mismatches).toEqual([]);
  });
});
