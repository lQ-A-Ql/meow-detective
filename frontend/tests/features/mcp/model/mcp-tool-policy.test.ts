import { describe, expect, it } from 'vitest';
import { toggleQueryTools } from '@/features/mcp/model/mcp-tool-policy';
import type { McpHostTool } from '@/types/mcpHost';

function tool(name: string, readOnly: boolean): McpHostTool {
  return {
    name,
    description: `${name} description`,
    inputSchema: { type: 'object' },
    outputSchema: { type: 'object' },
    readOnly,
    destructive: !readOnly,
  };
}

const catalogue: McpHostTool[] = [
  tool('forensics.search', true),
  tool('forensics.list_jobs', true),
  tool('forensics.extract_file', false),
];

describe('toggleQueryTools', () => {
  it('disables every read-only tool', () => {
    expect(toggleQueryTools([], catalogue, true)).toEqual([
      'forensics.search',
      'forensics.list_jobs',
    ]);
  });

  it('keeps existing disabled tools when disabling', () => {
    expect(toggleQueryTools(['custom.tool'], catalogue, true)).toEqual([
      'custom.tool',
      'forensics.search',
      'forensics.list_jobs',
    ]);
  });

  it('does not duplicate a read-only tool that is already disabled', () => {
    expect(toggleQueryTools(['forensics.search'], catalogue, true)).toEqual([
      'forensics.search',
      'forensics.list_jobs',
    ]);
  });

  it('never disables non-read-only tools', () => {
    expect(toggleQueryTools([], catalogue, true)).not.toContain('forensics.extract_file');
  });

  it('re-enables read-only tools while preserving non-read-only entries', () => {
    const disabled = ['forensics.search', 'forensics.list_jobs', 'forensics.extract_file'];

    expect(toggleQueryTools(disabled, catalogue, false)).toEqual(['forensics.extract_file']);
  });

  it('leaves unrelated disabled entries untouched when enabling', () => {
    expect(toggleQueryTools(['custom.tool'], catalogue, false)).toEqual(['custom.tool']);
  });

  it('does nothing to non-read-only entries when enabling', () => {
    expect(toggleQueryTools(['forensics.extract_file'], catalogue, false)).toEqual([
      'forensics.extract_file',
    ]);
  });

  it('returns a new array without mutating the input', () => {
    const disabled = Object.freeze(['forensics.search']) as unknown as string[];
    const result = toggleQueryTools(disabled, catalogue, false);

    expect(result).not.toBe(disabled);
    expect(disabled).toEqual(['forensics.search']);
    expect(result).toEqual([]);
  });

  it('returns the disabled entries when there are no tools', () => {
    expect(toggleQueryTools(['a', 'b'], [], true)).toEqual(['a', 'b']);
  });
});
