import assert from 'node:assert/strict';
import test from 'node:test';
import { highlightPieces } from '../../frontend/src/features/files/text-viewer/highlight-pieces.ts';

test('syntax and search ranges preserve every character of the source line', () => {
  const text = 'const answer = 42;';
  const pieces = highlightPieces(text, [{ start: 0, end: 5, kind: 'keyword' }, { start: 15, end: 17, kind: 'number' }], []);
  assert.equal(pieces.map((piece) => piece.text).join(''), text);
  assert.equal(pieces.filter((piece) => piece.kind === 'syntax').length, 2);
});

test('one search match spans several syntax tokens without duplicate match marks', () => {
  const pieces = highlightPieces('const value', [{ start: 0, end: 5, kind: 'keyword' }, { start: 6, end: 11, kind: 'variable' }],
    [{ start: 2, end: 9, matchIndex: 4, active: true }]);
  const matches = pieces.filter((piece) => piece.kind === 'search');
  assert.equal(matches.length, 1);
  assert.equal(matches[0].text, 'nst val');
  assert.equal(matches[0].active, true);
  assert.equal(pieces.map((piece) => piece.text).join(''), 'const value');
});

test('ranges are clipped to the actual line and empty lines retain their height', () => {
  assert.deepEqual(highlightPieces('', [], []), [{ kind: 'plain', start: 0, text: '\u00a0' }]);
  assert.equal(highlightPieces('abc', [{ start: -2, end: 20, kind: 'string' }], []).map((piece) => piece.text).join(''), 'abc');
});
