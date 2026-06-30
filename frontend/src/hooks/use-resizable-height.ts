/**
 * useResizableHeight - 可调整面板高度 Hook
 *
 * 支持垂直方向拖拽调整高度，包括：
 * - 最小/最大高度限制
 * - 持久化保存
 * - 平滑拖拽（向上拖拽增大高度，符合顶部手柄直觉）
 */

import { useCallback, useEffect, useState } from 'react';

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
      const saved = localStorage.getItem(storageKey);
      if (saved) {
        const parsed = parseInt(saved, 10);
        if (!isNaN(parsed)) {
          return Math.max(minHeight, Math.min(maxHeight, parsed));
        }
      }
    }
    return defaultHeight;
  });

  const [isResizing, setIsResizing] = useState(false);

  useEffect(() => {
    if (storageKey) {
      localStorage.setItem(storageKey, height.toString());
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

      const handleMouseUp = () => {
        setIsResizing(false);
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };

      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
    },
    [height, minHeight, maxHeight]
  );

  return { height, isResizing, onResizeStart };
}
