import assert from 'node:assert/strict';
import test from 'node:test';
import { actionButtonViolations } from '../guards/frontend-actions-policy.mjs';

const filename = 'frontend/src/features/files/components/Actions.tsx';

test('TypeScript AST distinguishes icon conditions from text conditions', () => {
  assert.equal(actionButtonViolations('const v = <Button>{busy ? <Spinner /> : <Icon />}</Button>;', filename).length, 0);
  assert.equal(actionButtonViolations('const v = <Button>{busy ? t("busy") : t("ready")}</Button>;', filename).length, 1);
});

test('nested JSX, arrow handlers and template literals retain text validation', () => {
  const content = 'const v = <Button onClick={() => value > 1} className={`x${value}`}><span>{title}</span></Button>;';
  assert.equal(actionButtonViolations(content, filename).length, 1);
  assert.equal(actionButtonViolations(content.replace('<Button ', '<Button shape="pill" '), filename).length, 0);
});

test('import aliases and asChild keep the same action shape policy', () => {
  const content = 'import { Button as Action } from "@/app/components/ui/button"; const v = <Action asChild><a>Open</a></Action>;';
  assert.equal(actionButtonViolations(content, filename).length, 1);
});

test('comments and badge counters do not become action labels', () => {
  assert.equal(actionButtonViolations('const v = <Button>{/* annotation */}<Icon /><Badge>{count}</Badge></Button>;', filename).length, 0);
});

test('dynamic action shapes only allow labels within the matching labeled branch', () => {
  const guarded = 'const v = <Button shape={actionShape}><Icon />{actionShape === "pill" ? title : null}</Button>;';
  assert.equal(actionButtonViolations(guarded, filename).length, 0);
  assert.equal(actionButtonViolations(guarded.replace(' : null', ' : title'), filename).length, 1);
});
