import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { CommandPalette, type CommandPaletteItem } from '@/app/components/CommandPalette';

function commands(run: () => void): CommandPaletteItem[] {
  return [
    { id: 'go:/', label: '案件概览', group: '前往', run },
    { id: 'go:/files', label: '文件浏览', group: '前往', run },
  ];
}

describe('CommandPalette', () => {
  it('opens on the keyboard shortcut, filters and runs the chosen command', () => {
    const run = vi.fn();
    render(<CommandPalette commands={commands(run)} />);

    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });

    const input = screen.getByRole('textbox');
    expect(screen.getByRole('option', { name: /案件概览/ })).toBeDefined();

    fireEvent.change(input, { target: { value: '文件' } });
    expect(screen.queryByRole('option', { name: /案件概览/ })).toBeNull();

    fireEvent.keyDown(input, { key: 'Enter' });
    expect(run).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('closes on Escape without running anything', () => {
    const run = vi.fn();
    render(<CommandPalette commands={commands(run)} />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    fireEvent.keyDown(window, { key: 'Escape' });

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(run).not.toHaveBeenCalled();
  });
});
