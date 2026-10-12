import { useEffect, type KeyboardEvent, type RefObject } from 'react';
import type { TextViewerModel } from './use-text-viewer-model';

export function useTextSearchKeyboard(model: TextViewerModel, viewportRef: RefObject<HTMLDivElement>) {
  // Capture at the owning window before a containing dialog handles Escape.
  useEffect(() => {
    const owner = viewportRef.current?.ownerDocument.defaultView;
    if (!owner || !model.query) return;
    const clear = (event: globalThis.KeyboardEvent) => {
      if (event.key !== 'Escape' || event.isComposing) return;
      const region = viewportRef.current?.closest('[data-slot="text-viewer"]')?.querySelector('[role="search"]');
      const target = event.target;
      if (!region || !target || !('nodeType' in target) || !region.contains(target as Node)) return;
      event.preventDefault();
      event.stopPropagation();
      model.setQuery('');
    };
    owner.addEventListener('keydown', clear, true);
    return () => owner.removeEventListener('keydown', clear, true);
  }, [model.query, model.setQuery, viewportRef]);

  return (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      model.moveMatch(event.shiftKey ? -1 : 1);
    }
  };
}
