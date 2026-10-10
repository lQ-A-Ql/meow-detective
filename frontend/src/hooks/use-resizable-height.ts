/**
 * useResizableHeight - 可调整面板高度 Hook
 *
 * 支持垂直方向拖拽调整高度，包括：
 * - 最小/最大高度限制
 * - 持久化保存
 * - 平滑拖拽（向上拖拽增大高度，符合顶部手柄直觉）
 */

import { useCallback, useEffect, useRef, useState } from 'react';

interface UseResizableHeightOptions {
  /** 默认高度 */
  defaultHeight: number;
  /** 最小高度 */
  minHeight?: number;
  /** 最大高度 */
  maxHeight?: number;
  /** localStorage key */
  storageKey?: string;
}

interface UseResizableHeightReturn {
  /** 当前高度 */
  height: number;
  /** 是否正在拖拽 */
  isResizing: boolean;
  /** 拖拽开始回调 */
  onResizeStart: (e: React.MouseEvent) => void;
}

export function useResizableHeight({
  defaultHeight,
  minHeight = 120,
  maxHeight = 600,
  storageKey,
}: UseResizableHeightOptions): UseResizableHeightReturn {
  const [height, setHeight] = useState(() => {
    if (storageKey) {
      try {
        const saved = localStorage.getItem(storageKey);
        if (saved) {
          const parsed = parseInt(saved, 10);
          if (!isNaN(parsed)) {
            return Math.max(minHeight, Math.min(maxHeight, parsed));
          }
        }
      } catch {
        // Storage can be unavailable (disabled or private mode); fall back to the default.
      }
    }
    return defaultHeight;
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

  useEffect(() => {
    if (!storageKey) {
      return;
    }
    try {
      localStorage.setItem(storageKey, height.toString());
    } catch {
      // Persistence is best-effort; a blocked write must not break dragging.
    }
  }, [height, storageKey]);

  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsResizing(true);

      const startY = e.clientY;
      const startHeight = height;

      const handleMouseMove = (e: MouseEvent) => {
        const diff = startY - e.clientY;
        const newHeight = Math.max(minHeight, Math.min(maxHeight, startHeight + diff));
        setHeight(newHeight);
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
    [height, minHeight, maxHeight]
  );

  return { height, isResizing, onResizeStart };
}
