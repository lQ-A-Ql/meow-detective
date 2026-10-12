/** Synthetic content for local text-preview interaction review only. */
if (import.meta.env.PROD) throw new Error('Text viewer review fixtures are development-only');

const tailLines = Array.from({ length: 1376 }, (_, i) => `# synthetic configuration item ${i + 1}`);
tailLines[18] = '# password option placeholder';
tailLines[1374] = 'requirepass synthetic-value';
const boundaryLines = Array.from({ length: 1002 }, (_, i) => `# boundary item ${i + 1}`);
boundaryLines[999] = 'needle before';
boundaryLines[1000] = 'needle after';

export const textViewerReview = {
  tail: { content: tailLines.join('\n'), query: 'pass' },
  boundary: { content: boundaryLines.join('\r\n'), query: 'needle' },
  horizontal: { content: '中文\t' + 'x'.repeat(500) + ' needle nearby\nshort row', query: 'needle' },
  segment: { content: 'x'.repeat(8190) + 'needle end', query: 'needle' },
  sameLine: { content: 'pass and PASS\nlast pass', query: 'pass' },
};
