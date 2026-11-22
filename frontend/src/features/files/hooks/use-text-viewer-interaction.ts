import { useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCopyText } from '@/hooks/use-copy-text';

export function useTextViewerInteraction(content: string) {
  const { t } = useTranslation();
  const rootRef = useRef<HTMLDivElement>(null);
  const sequence = useRef(0);
  const [notice, setNotice] = useState<{ message: string; sequence: number }>();
  const showNotice = (message: string) => {
    sequence.current += 1;
    setNotice({ message, sequence: sequence.current });
  };
  const copy = useCopyText(content, {
    onSuccess: () => showNotice(t('textViewer.copiedAll', { count: content.length })),
    onError: () => showNotice(t('textViewer.copyFailed')),
  });
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'a') return;
    const target = event.target as HTMLElement;
    if (target.closest('input, textarea, [contenteditable="true"]')) return;
    event.preventDefault();
    copy();
  };
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('input, textarea, button, [contenteditable="true"]')) return;
    rootRef.current?.focus();
  };
  return { rootRef, notice, onKeyDown, onPointerDown, dismissNotice: () => setNotice(undefined) };
}
