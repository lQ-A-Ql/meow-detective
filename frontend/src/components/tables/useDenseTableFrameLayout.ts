import { useLayoutEffect, useRef } from 'react';

export function useDenseTableFrameLayout(initialHeight: string | undefined, maximumHeight: string) {
  const frameRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const frame = frameRef.current;
    if (!frame || initialHeight === undefined) return;

    frame.style.height = initialHeight;
    const table = frame.querySelector<HTMLTableElement>('[data-slot="table"]');
    if (!table) return;

    const sections = [
      table,
      frame.querySelector<HTMLElement>('[data-slot="dense-table-filter"]'),
      frame.querySelector<HTMLElement>('[data-slot="dense-table-frame-header"]'),
    ].filter((element): element is HTMLElement => element !== null);

    const measureHeight = () => {
      if (table.getClientRects().length === 0) return;

      const style = window.getComputedStyle(frame);
      const frameSpacing =
        Number.parseFloat(style.borderTopWidth) +
        Number.parseFloat(style.borderBottomWidth) +
        Number.parseFloat(style.paddingTop) +
        Number.parseFloat(style.paddingBottom);
      const contentHeight = sections.reduce((height, section) => height + section.getBoundingClientRect().height, 0);
      frame.style.height = `min(${Math.ceil(contentHeight + frameSpacing)}px, ${maximumHeight})`;
    };

    const observer = new ResizeObserver(measureHeight);
    sections.forEach((section) => observer.observe(section));
    measureHeight();
    return () => observer.disconnect();
  });

  return frameRef;
}
