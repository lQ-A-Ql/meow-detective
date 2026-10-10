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
    <nav aria-label={ariaLabel} className={cn('border-r border-forensics-line bg-forensics-soft p-2', className)}>
      <ol role="tablist" aria-orientation="vertical" className="flex flex-col gap-1">
        {steps.map((step, index) => {
          const isCurrent = index === currentIndex;
          const isDone = index < currentIndex;
          const isLocked = index > currentIndex;
          return (
            <li key={step.id} role="presentation" className="min-w-0">
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
                  'flex w-full min-w-0 items-center gap-2 px-2 py-1.5 text-[11px] outline-none focus-visible:ring-1 focus-visible:ring-forensics-red',
                  isLocked ? 'text-forensics-ink-4' : 'cursor-pointer text-forensics-ink-2 hover:bg-forensics-hover',
                  'data-[state=current]:text-forensics-red-ink',
                )}
              >
                <span className="w-5 shrink-0 font-mono text-[10px] text-forensics-ink-3">
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
