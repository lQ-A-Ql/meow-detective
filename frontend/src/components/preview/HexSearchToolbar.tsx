import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { cn } from '@/app/components/ui/utils';
import { hexPatternByteLength, hexSearchErrorKey, isHexSearchActive } from '@/features/files/hooks/use-hex-search';
import type { HexSearchFileState } from '@/stores/hex-search-store';
import { formatBytes } from '@/lib/format-bytes';
import type { HexSearchMode } from '@/types/models';

interface Props {
  fileId?: string;
  state?: HexSearchFileState;
  onStart: (mode: HexSearchMode, pattern: string) => void;
  onCancel: () => void;
  onReset: () => void;
  onPrevious: () => void;
  onNext: () => void;
}

export function HexSearchToolbar({ fileId, state, onStart, onCancel, onReset, onPrevious, onNext }: Props) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<HexSearchMode>(state?.mode ?? 'hex');
  const [pattern, setPattern] = useState(state?.pattern ?? '');
  const [invalid, setInvalid] = useState(false);

  useEffect(() => {
    setMode(state?.mode ?? 'hex');
    setPattern(state?.pattern ?? '');
    setInvalid(false);
  }, [fileId, state?.mode, state?.pattern]);

  const active = isHexSearchActive(state?.phase);
  const matches = state?.matches.length ?? 0;
  const submit = () => {
    if (hexPatternByteLength(mode, pattern) === undefined) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    onStart(mode, pattern);
  };
  const clear = () => {
    setPattern('');
    setInvalid(false);
    onReset();
  };

  return (
    <div
      className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-forensics-border bg-forensics-panel px-3 py-1 text-[10px] text-forensics-muted"
      role="search"
      aria-label={t('preview.hex.search.label')}
    >
      <div className="flex items-center" role="group" aria-label={t('preview.hex.search.label')}>
        {(['hex', 'ascii'] as const).map((value) => (
          <Button
            key={value}
            type="button"
            shape="pill"
            variant="forensicsOutline"
            aria-pressed={mode === value}
            onClick={() => setMode(value)}
            className={cn(
              'h-5 min-h-0 w-auto gap-0 rounded-none border px-1.5 py-0 font-mono text-[10px] leading-none uppercase',
              mode === value
                ? 'border-forensics-border bg-forensics-panel-strong text-forensics-ink'
                : 'border-transparent text-forensics-muted hover:bg-forensics-hover',
            )}
          >
            {value === 'hex' ? t('preview.hex.search.hexMode') : t('preview.hex.search.asciiMode')}
          </Button>
        ))}
      </div>
      <Input
        value={pattern}
        onChange={(event) => {
          setPattern(event.target.value);
          setInvalid(false);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            submit();
          } else if (event.key === 'Escape') {
            event.preventDefault();
            clear();
          }
        }}
        aria-label={t('preview.hex.search.label')}
        aria-invalid={invalid || undefined}
        placeholder={mode === 'hex' ? t('preview.hex.search.hexPlaceholder') : t('preview.hex.search.asciiPlaceholder')}
        variant="mono"
        inputSize="inline"
        className="w-44"
      />
      <Tooltip content={t('preview.hex.search.start')}>
        <Button
          aria-label={t('preview.hex.search.start')}
          type="button"
          variant="forensicsOutline"
          size="compact"
          onClick={submit}
          disabled={active}
        >
          <ActionIcon name="search" />
        </Button>
      </Tooltip>
      <IconButton
        label={t('preview.hex.search.previousMatch')}
        size="iconXs"
        disabled={matches === 0}
        onClick={onPrevious}
      >
        <ActionIcon name="chevron-up" />
      </IconButton>
      <IconButton label={t('preview.hex.search.nextMatch')} size="iconXs" disabled={matches === 0} onClick={onNext}>
        <ActionIcon name="chevron-down" />
      </IconButton>
      {pattern ? (
        <IconButton label={t('preview.hex.search.clear')} size="iconXs" onClick={clear}>
          <ActionIcon name="close" />
        </IconButton>
      ) : null}
      {active ? (
        <span className="whitespace-nowrap">
          {state?.phase === 'cancelling'
            ? t('preview.hex.search.cancelling')
            : state?.totalBytes
              ? t('preview.hex.search.scanned', {
                  scanned: formatBytes(state.scannedBytes),
                  total: formatBytes(state.totalBytes),
                })
              : t('preview.hex.search.scanning')}
        </span>
      ) : matches > 0 ? (
        <span role="status" className="whitespace-nowrap font-mono text-forensics-ink">
          {t('preview.hex.search.matches', { current: (state?.activeIndex ?? 0) + 1, total: matches })}
        </span>
      ) : null}
      {state?.truncated ? <span>{t('preview.hex.search.truncated')}</span> : null}
      {!invalid && !active && state?.phase === 'completed' && matches === 0 ? (
        <span>{t('preview.hex.search.noMatches')}</span>
      ) : null}
      {invalid ? <span className="text-forensics-error-text">{t('preview.hex.search.invalidPattern')}</span> : null}
      {state?.error ? (
        <span className="text-forensics-error-text">{t(hexSearchErrorKey(state.error), { code: state.error })}</span>
      ) : null}
      {active ? (
        <IconButton label={t('preview.hex.search.cancel')} size="iconXs" onClick={onCancel}>
          <ActionIcon name="close" />
        </IconButton>
      ) : null}
    </div>
  );
}
