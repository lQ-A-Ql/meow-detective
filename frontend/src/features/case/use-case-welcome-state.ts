import { useState, type FormEvent } from 'react';
import type { RecentCase } from '@/types/models';
import type { CaseWelcomeFormsProps } from './components/case-welcome-types';

export function useCaseWelcomeState(props: CaseWelcomeFormsProps) {
  const [mode, setMode] = useState<'create' | 'open'>('create');
  const [recentOpen, setRecentOpen] = useState(false);
  const busy = props.createPending || props.openPending;
  const canCreate = !busy && Boolean(props.caseRoot.trim() && props.caseName.trim());
  const canOpen = !busy && Boolean(props.openCasePath.trim());

  return {
    mode, recentOpen, setRecentOpen, busy, canCreate, canOpen,
    selectMode(value: string) {
      if (value === 'create' || value === 'open') setMode(value);
    },
    submitCreate(event: FormEvent) {
      event.preventDefault();
      if (canCreate) props.onCreateCase();
    },
    submitOpen(event: FormEvent) {
      event.preventDefault();
      if (canOpen) props.onOpenCase(props.openCasePath.trim());
    },
    openRecent(item: RecentCase) {
      setRecentOpen(false);
      props.onOpenCase(item.caseRoot);
    },
    deleteRecent(item: RecentCase) {
      setRecentOpen(false);
      props.onRequestDeleteCase(item);
    },
  };
}

export type CaseWelcomeState = ReturnType<typeof useCaseWelcomeState>;
