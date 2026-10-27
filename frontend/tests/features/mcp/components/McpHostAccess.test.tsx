import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { McpHostAccessView } from '@/features/mcp/components/McpHostAccessView';
import { toggleQueryTools } from '@/features/mcp/model/mcp-tool-policy';
import i18n from '@/i18n';

describe('MCP host access', () => {
  it('keeps write tool policy unchanged when query tools are enabled or disabled', () => {
    const tools = [
      { name: 'forensics.read_file', description: '', inputSchema: { type: 'object' }, outputSchema: { type: 'object' }, readOnly: true, destructive: false },
      { name: 'forensics.extract_file', description: '', inputSchema: { type: 'object' }, outputSchema: { type: 'object' }, readOnly: false, destructive: true },
    ];
    expect(toggleQueryTools(tools.map((tool) => tool.name), tools, false)).toEqual(['forensics.extract_file']);
    expect(toggleQueryTools([], tools, true)).toEqual(['forensics.read_file']);
  });

  it('uses labelled grant controls and dispatches access choices', () => {
    let name = '';
    let evidence = false;
    let submissions = 0;
    render(<McpHostAccessView t={i18n.t} clients={[]} name="" setName={(value) => { name = value; }}
      hours="24" setHours={() => {}} readEvidence={false} setReadEvidence={(value) => { evidence = value; }} dataSourceIds="" setDataSourceIds={() => {}}
      closeCreated={() => {}} busy={false} canCreate create={() => { submissions += 1; }} revoke={() => {}}
      onCopy={() => {}} copied={false} copyError={false} />);
    fireEvent.change(screen.getByLabelText('客户端名称'), { target: { value: 'Investigation' } });
    fireEvent.click(screen.getByRole('checkbox', { name: '允许读取文件内容' }));
    fireEvent.click(screen.getByRole('button', { name: '授权当前案件' }));
    expect(name).toBe('Investigation');
    expect(evidence).toBe(true);
    expect(submissions).toBe(1);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
