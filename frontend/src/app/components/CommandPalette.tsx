import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Input } from '@/app/components/ui/input';
import { cn } from '@/app/components/ui/utils';

export interface CommandPaletteItem {
  id: string;
  label: string;
  group: string;
  run: () => void;
}

/**
 * Keyboard entry point for the whole workbench. It is a pure view: the shell owns the command
 * list, the palette owns filtering, focus and dismissal.
 */
export function CommandPalette({ commands }: { commands: CommandPaletteItem[] }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setOpen((current) => !current);
        setQuery('');
        setActive(0);
      }
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? commands.filter((command) => command.label.toLowerCase().includes(needle)) : commands;
  }, [commands, query]);

  if (!open) return null;
  const activeIndex = filtered.length ? Math.min(active, filtered.length - 1) : 0;

  const runAt = (index: number) => {
    const command = filtered[index];
    if (!command) return;
    setOpen(false);
    command.run();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-forensics-text/10 pt-24" onMouseDown={() => setOpen(false)}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t('commandPalette.label')}
        className="w-full max-w-lg border border-forensics-border-strong bg-forensics-surface shadow-lg"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <Input
          autoFocus
          value={query}
          onChange={(event) => { setQuery(event.target.value); setActive(0); }}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown') { event.preventDefault(); setActive((i) => Math.min(i + 1, filtered.length - 1)); }
            if (event.key === 'ArrowUp') { event.preventDefault(); setActive((i) => Math.max(i - 1, 0)); }
            if (event.key === 'Enter') { event.preventDefault(); runAt(activeIndex); }
          }}
          placeholder={t('commandPalette.placeholder')}
          aria-label={t('commandPalette.placeholder')}
          className="h-11 w-full rounded-none border-0 border-b border-forensics-border bg-transparent px-4 text-[13px] text-forensics-text"
        />
        <div role="listbox" aria-label={t('commandPalette.label')} className="max-h-80 overflow-y-auto scrollbar-none py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-[12px] text-forensics-muted">{t('commandPalette.empty')}</div>
          ) : filtered.map((command, index) => (
            <div
              key={command.id}
             
             
             
              role="option"
              aria-selected={index === activeIndex}
              onMouseEnter={() => setActive(index)}
              onClick={() => runAt(index)}
              className={cn(
                'flex h-auto w-full items-center justify-between gap-3 rounded-none px-4 py-2 text-left text-[12px] text-forensics-text',
                index === activeIndex ? 'bg-forensics-hover' : 'bg-transparent',
              )}
            >
              <span>{command.label}</span>
              <span className="text-[10px] uppercase tracking-wide text-forensics-muted-light">{command.group}</span>
            </div>
          ))}
        </div>
        <div className="border-t border-forensics-border px-4 py-2 text-[10px] text-forensics-muted-light">{t('commandPalette.hint')}</div>
      </div>
    </div>
  );
}
