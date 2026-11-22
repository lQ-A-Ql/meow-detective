import { useEffect, useState, type KeyboardEvent } from 'react';
import { hexPatternByteLength, hexSearchErrorKey, isHexSearchActive } from './use-hex-search';
import type { HexSearchFileState } from '@/stores/hex-search-store';
import type { HexSearchMode } from '@/types/models';

export interface HexSearchToolbarOptions {
  fileId?: string;
  state?: HexSearchFileState;
  onStart: (mode: HexSearchMode, pattern: string) => void;
  onReset: () => void;
  onCancel: () => void;
  onPrevious: () => void;
  onNext: () => void;
}

export function useHexSearchToolbarModel(options: HexSearchToolbarOptions) {
  const { fileId, state, onStart, onReset } = options;
  const [mode, setMode] = useState<HexSearchMode>(state?.mode ?? 'hex');
  const [pattern, setPattern] = useState(state?.pattern ?? '');
  const [invalid, setInvalid] = useState(false);
  useEffect(() => {
    setMode(state?.mode ?? 'hex');
    setPattern(state?.pattern ?? '');
    setInvalid(false);
  }, [fileId, state?.mode, state?.pattern]);
  const submit = () => {
    const valid = hexPatternByteLength(mode, pattern) !== undefined;
    setInvalid(!valid);
    if (valid) onStart(mode, pattern);
  };
  const clear = () => {
    setPattern('');
    setInvalid(false);
    onReset();
  };
  const changePattern = (value: string) => {
    setPattern(value);
    setInvalid(false);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter' || event.key === 'Escape') {
      event.preventDefault();
      if (event.key === 'Enter') submit();
      else clear();
    }
  };
  return {
    ...options,
    mode,
    setMode,
    pattern,
    changePattern,
    invalid,
    submit,
    clear,
    onKeyDown,
    active: isHexSearchActive(state?.phase),
    matches: state?.matches.length ?? 0,
    errorKey: state?.error ? hexSearchErrorKey(state.error) : undefined,
  };
}

export type HexSearchToolbarModel = ReturnType<typeof useHexSearchToolbarModel>;
