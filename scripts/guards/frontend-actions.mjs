import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { actionButtonViolations } from './frontend-actions-policy.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));

async function checkDirectory(directory) {
  const failures = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (['fixtures', 'fixture', '__tests__', 'test'].includes(entry.name)) continue;
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) failures.push(...await checkDirectory(filename));
    else if (/\.tsx?$/.test(entry.name) && !/\.(test|spec)\.tsx?$/.test(entry.name)) {
      const relative = path.relative(root, filename).replaceAll('\\', '/');
      for (const violation of actionButtonViolations(await readFile(filename, 'utf8'), relative)) {
        failures.push(`${relative}:${violation.line}: ${violation.message}`);
      }
    }
  }
  return failures;
}

const failures = await checkDirectory(path.join(root, 'frontend/src'));
if (failures.length > 0) {
  process.stderr.write(`${failures.join('\n')}\n`);
  process.exitCode = 1;
}
