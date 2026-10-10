import type { KeyboardEvent } from 'react';
import { cn } from '@/app/components/ui/utils';

export interface WizardStep {
  id: string;
  label: string;
}

/**
 * Wizard step navigation. It owns its element instead of borrowing the action-button primitive:
 * a step is navigation state (role=tab in a vertical tablist), it is always full width, and it
 * never collapses to a circular hit area the way an icon button does.
 */
export function StepRail({
  steps,
  currentId,
  onSelect,
  ariaLabel,
  className,
}: {
  steps: WizardStep[];
  currentId: string;
  onSelect: (id: string) => void;
  ariaLabel: string;
  className?: string;
}) {
  const currentIndex = steps.findIndex((step) => step.id === currentId);
  return (
    <nav aria-label={ariaLabel} className={cn('h-full w-full min-w-0 border-r border-forensics-line bg-forensics-soft', className)}>
      <ol role="tablist" aria-orientation="vertical" className="flex h-full flex-col">
        {steps.map((step, index) => {
          const isCurrent = index === currentIndex;
          const isDone = index < currentIndex;
          const isLocked = index > currentIndex;
          return (
            <li key={step.id} role="presentation" className="flex min-h-0 min-w-0 flex-1">
              <div
                role="tab"
                aria-selected={isCurrent}
                aria-disabled={isLocked || undefined}
                tabIndex={isCurrent ? 0 : -1}
                data-state={isCurrent ? 'current' : isDone ? 'done' : 'todo'}
                onClick={() => {
                  if (!isLocked) onSelect(step.id);
                }}
                onKeyDown={(event: KeyboardEvent<HTMLDivElement>) => {
                  if (event.key !== 'Enter' && event.key !== ' ') return;
                  event.preventDefault();
                  if (!isLocked) onSelect(step.id);
                }}
                className={cn(
                  'relative flex h-full w-full min-w-0 items-center gap-3 border-l-2 px-4 text-[12px] outline-none focus-visible:ring-1 focus-visible:ring-forensics-red',
                  isLocked ? 'border-l-transparent text-forensics-ink-4' : 'cursor-pointer text-forensics-ink-2 hover:bg-forensics-hover',
                  isDone && 'border-l-forensics-ink-4',
                  isCurrent && 'border-l-forensics-red bg-forensics-red-tint text-forensics-red-ink',
                )}
              >
                <span className="w-6 shrink-0 font-mono text-[12px] text-forensics-ink-3">
                  {String(index + 1).padStart(2, '0')}
                </span>
                <span className="min-w-0 flex-1 truncate">{step.label}</span>
              </div>
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
