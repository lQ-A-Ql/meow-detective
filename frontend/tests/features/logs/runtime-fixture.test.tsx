import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { LogWorkspaceView } from '@/features/logs/components/LogWorkspaceView';
import { emptyLogBuffer, mergeLogSnapshot } from '@/features/logs/log-buffer';
import type { ApplicationLogSnapshot } from '@/types/applicationLog';
import type { LogWorkspaceModel } from '@/features/logs/use-log-workspace-model';

const fixturePath = process.env.FORENSICS_LOG_UI_FIXTURE;
const suite = fixturePath ? describe : describe.skip;

suite('actual Rust application log response', () => {
  function snapshot(): ApplicationLogSnapshot {
    return JSON.parse(readFileSync(fixturePath!, 'utf8')) as ApplicationLogSnapshot;
  }

  it('keeps all three real levels and deduplicates repeated responses', () => {
    const input = snapshot();
    const first = mergeLogSnapshot(emptyLogBuffer(), input);
    const second = mergeLogSnapshot(first, input);
    expect(second.rows.info).toEqual(first.rows.info);
    expect(second.rows.warning[0].message).toBe('warning');
    expect(second.rows.error[0].message).toBe('error');
    expect(second.forward).toHaveLength(3);
  });

  it('keeps query boundaries when the real service returns empty filtered pages', () => {
    const input = JSON.parse(readFileSync(join(dirname(fixturePath!), 'empty.json'), 'utf8')) as ApplicationLogSnapshot;
    const buffer = mergeLogSnapshot(emptyLogBuffer(), input);
    expect(Object.values(buffer.rows).flat()).toHaveLength(0);
    expect(buffer.forward).toEqual(input.pages.map((page) => page.forwardCursor));
    expect(buffer.backward).toEqual(input.pages.map((page) => page.backwardCursor));
    const populated = mergeLogSnapshot(emptyLogBuffer(), snapshot());
    expect(mergeLogSnapshot(populated, input).rows).toBe(populated.rows);
  });

  it('renders INFO, WARNING and ERROR in their own panels and routes selection', () => {
    const input = snapshot();
    let selected = 0;
    let paused = false;
    const model: LogWorkspaceModel = {
      buffer: mergeLogSnapshot(emptyLogBuffer(), input),
      filter: { text: '', target: '', caseId: '', dataSourceId: '', jobId: '' },
      setFilter() {},
      paused,
      setPaused(value) {
        paused = typeof value === 'function' ? value(paused) : value;
      },
      selected: undefined,
      select(record) {
        selected = record.sequence;
      },
      closeDetails() {},
      clearDisplay() {},
      loadHistory: async () => {},
      reopenWriter: async () => {},
      copySelected: async () => {},
      copyStatus: undefined,
      busy: false,
      error: undefined,
    };
    render(<LogWorkspaceView model={model} />);
    expect(within(screen.getByRole('region', { name: 'INFO' })).getAllByText('service log')).toHaveLength(2);
    expect(within(screen.getByRole('region', { name: 'WARNING' })).getByText('warning')).toBeInTheDocument();
    expect(within(screen.getByRole('region', { name: 'ERROR' })).getByText('error')).toBeInTheDocument();
    fireEvent.click(screen.getByText('warning'));
    expect(selected).toBe(input.pages[1].records[0].sequence);
    fireEvent.click(screen.getByRole('button', { name: '暂停显示' }));
    expect(paused).toBe(true);
  });
});
