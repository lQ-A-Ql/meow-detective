import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { CaseWelcomeForms } from '@/features/case/components/CaseActions';
import type { CaseWelcomeFormsProps } from '@/features/case/components/CaseActions';

function baseWelcomeProps(overrides: Partial<CaseWelcomeFormsProps> = {}): CaseWelcomeFormsProps {
  return {
    caseRoot: '',
    setCaseRoot: vi.fn(),
    caseName: '',
    setCaseName: vi.fn(),
    onCreateCase: vi.fn(),
    createPending: false,
    createError: null,
    openCasePath: '',
    setOpenCasePath: vi.fn(),
    onOpenCase: vi.fn(),
    openPending: false,
    openError: null,
    recentCases: [],
    onRequestDeleteCase: vi.fn(),
    ...overrides,
  };
}

describe('CaseWelcomeForms', () => {
  it('disables create button when case root or name is missing', () => {
    render(<CaseWelcomeForms {...baseWelcomeProps({ caseRoot: '', caseName: '' })} />);

    const createButton = screen.getByRole('button', { name: '创建案件' }) as HTMLButtonElement;
    expect(createButton.disabled).toBe(true);
  });

  it('enables create button and invokes onCreateCase when fields are filled', () => {
    const onCreateCase = vi.fn();
    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({ caseRoot: 'C:\\cases', caseName: 'case-1', onCreateCase })}
      />,
    );

    const createButton = screen.getByRole('button', { name: '创建案件' }) as HTMLButtonElement;
    expect(createButton.disabled).toBe(false);
    fireEvent.click(createButton);
    expect(onCreateCase).toHaveBeenCalledTimes(1);
  });

  it('shows creating label and create error message', () => {
    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({
          caseRoot: 'C:\\cases',
          caseName: 'case-1',
          createPending: true,
          createError: '目录已存在',
        })}
      />,
    );

    expect(screen.getByRole('button', { name: /^创建案件$|创建中/ })).toBeDefined();
    expect(screen.getByText('目录已存在')).toBeDefined();
  });

  it('disables open button when path is empty and calls onOpenCase with the path otherwise', () => {
    const onOpenCase = vi.fn();
    const { rerender } = render(<CaseWelcomeForms {...baseWelcomeProps({ openCasePath: '' })} />);
    fireEvent.mouseDown(screen.getByRole('tab', { name: '打开已有案件' }), { button: 0, ctrlKey: false });

    let openButton = screen.getByRole('button', { name: '打开案件' }) as HTMLButtonElement;
    expect(openButton.disabled).toBe(true);

    rerender(<CaseWelcomeForms {...baseWelcomeProps({ openCasePath: 'C:\\cases\\case-1', onOpenCase })} />);
    openButton = screen.getByRole('button', { name: '打开案件' }) as HTMLButtonElement;
    expect(openButton.disabled).toBe(false);
    fireEvent.click(openButton);
    expect(onOpenCase).toHaveBeenCalledWith('C:\\cases\\case-1');
  });

  it('shows open error message', () => {
    render(<CaseWelcomeForms {...baseWelcomeProps({ openError: '路径无效' })} />);
    fireEvent.mouseDown(screen.getByRole('tab', { name: '打开已有案件' }), { button: 0, ctrlKey: false });
    expect(screen.getByText('路径无效')).toBeDefined();
  });

  it('keeps the landing page free of explanatory copy and empty history', () => {
    render(<CaseWelcomeForms {...baseWelcomeProps({ recentCases: [] })} />);
    expect(screen.getByRole('heading', { name: 'Meow~Detective' })).toBeDefined();
    expect(screen.queryByText(/当前没有活动案件/)).not.toBeInTheDocument();
    expect(screen.queryByText(/这里会保留最近打开过的案件/)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '最近打开案件' })).not.toBeInTheDocument();
  });

  it('renders recent cases and opens one on click', () => {
    const onOpenCase = vi.fn();
    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({
          recentCases: [
            { caseRoot: 'C:\\cases\\case-1', name: 'Case One', openedAt: '2026-06-01' },
          ],
          onOpenCase,
        })}
      />,
    );

    expect(screen.queryByText('Case One')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '最近打开案件' }));
    expect(screen.getByRole('dialog')).toBeDefined();
    expect(screen.getByText('Case One')).toBeDefined();
    fireEvent.click(screen.getByText('Case One'));
    expect(onOpenCase).toHaveBeenCalledWith('C:\\cases\\case-1');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('requests deletion of a recent case without opening it', () => {
    const onRequestDeleteCase = vi.fn();
    const onOpenCase = vi.fn();

    render(
      <CaseWelcomeForms
        {...baseWelcomeProps({
          recentCases: [
            { caseRoot: 'C:\\cases\\case-1', name: 'Case One', openedAt: '2026-06-01' },
          ],
          onRequestDeleteCase,
          onOpenCase,
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '最近打开案件' }));
    fireEvent.click(screen.getByRole('button', { name: '删除案件' }));
    expect(onRequestDeleteCase).toHaveBeenCalledWith({
      caseRoot: 'C:\\cases\\case-1',
      name: 'Case One',
      openedAt: '2026-06-01',
    });
    expect(onOpenCase).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('binds each visible deletion action to its own case record', () => {
    const onRequestDeleteCase = vi.fn();
    const onOpenCase = vi.fn();
    const cases = [
      { caseRoot: 'D:/cases/' + 'long-parent-directory/'.repeat(12) + 'first', name: 'Same name', openedAt: '2026-10-19' },
      { caseRoot: 'E:/cases/second', name: 'Same name', openedAt: '2026-10-18' },
    ];
    render(<CaseWelcomeForms {...baseWelcomeProps({ recentCases: cases, onRequestDeleteCase, onOpenCase })} />);
    fireEvent.click(screen.getByRole('button', { name: '最近打开案件' }));
    const actions = screen.getAllByRole('button', { name: '删除案件' });
    expect(actions).toHaveLength(2);
    fireEvent.click(actions[1]);
    expect(onRequestDeleteCase).toHaveBeenCalledExactlyOnceWith(cases[1]);
    expect(onOpenCase).not.toHaveBeenCalled();
  });
});
