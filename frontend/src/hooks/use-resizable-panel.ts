/**
 * use-resizable-panel - 可调整面板宽度 Hook
 *
 * 支持拖拽调整面板宽度，包括：
 * - 最小/最大宽度限制
 * - 持久化保存
 * - 平滑拖拽
 */

import { useCallback, useEffect, useRef, useState } from 'react';

interface UseResizablePanelOptions {
  /** 默认宽度 */
  defaultWidth: number;
  /** 最小宽度 */
  minWidth?: number;
  /** 最大宽度 */
  maxWidth?: number;
  /** localStorage key */
  storageKey?: string;
}

interface UseResizablePanelReturn {
  /** 当前宽度 */
  width: number;
  /** 是否正在拖拽 */
  isResizing: boolean;
  /** 拖拽开始回调 */
  onResizeStart: (e: React.MouseEvent) => void;
}

export function useResizablePanel({
  defaultWidth,
  minWidth = 160,
  maxWidth = 400,
  storageKey,
}: UseResizablePanelOptions): UseResizablePanelReturn {
  const [width, setWidth] = useState(() => {
    if (storageKey) {
      try {
        const saved = localStorage.getItem(storageKey);
        if (saved) {
          const parsed = parseInt(saved, 10);
          if (!isNaN(parsed)) {
            return Math.max(minWidth, Math.min(maxWidth, parsed));
          }
        }
      } catch {
        // Storage can be unavailable (disabled or private mode); fall back to the default.
      }
    }
    return defaultWidth;
  });

  const [isResizing, setIsResizing] = useState(false);
  /**
   * Cleanup for the document listeners installed while dragging. A component that unmounts
   * mid-drag (panel closed, route change) never receives mouseup, which previously left two
   * document listeners attached to dead setState callbacks.
   */
  const dragCleanupRef = useRef<(() => void) | undefined>(undefined);

  useEffect(() => () => {
    dragCleanupRef.current?.();
  }, []);

  // 保存宽度
  useEffect(() => {
    if (!storageKey) {
      return;
    }
    try {
      localStorage.setItem(storageKey, width.toString());
    } catch {
      // Persistence is best-effort; a blocked write must not break dragging.
    }
  }, [width, storageKey]);

  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsResizing(true);

      const startX = e.clientX;
      const startWidth = width;

      const handleMouseMove = (e: MouseEvent) => {
        const diff = e.clientX - startX;
        const newWidth = Math.max(minWidth, Math.min(maxWidth, startWidth + diff));
        setWidth(newWidth);
      };

      const stopListening = () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
        dragCleanupRef.current = undefined;
      };

      const handleMouseUp = () => {
        setIsResizing(false);
        stopListening();
      };

      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      dragCleanupRef.current = stopListening;
    },
    [width, minWidth, maxWidth]
  );

  return { width, isResizing, onResizeStart };
}
