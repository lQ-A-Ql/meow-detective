/**
 * TreeContextMenu - 树右键菜单组件
 *
 * 提供文件树节点的右键菜单功能：
 * - 展开/折叠全部
 * - 复制路径
 * - 在时间线中查看
 * - 提取文件
 * - 属性
 */

import { useEffect, useRef } from 'react';
import {
  FolderOpen,
  FolderMinus,
  Copy,
  Clock,
  Download,
  Info,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { Button } from '@/app/components/ui/button';

interface ContextMenuItem {
  label: string;
  icon: LucideIcon;
  shortcut?: string;
  action: () => void;
  divider?: boolean;
  disabled?: boolean;
}

interface TreeContextMenuProps {
  /** 菜单 X 坐标 */
  x: number;
  /** 菜单 Y 坐标 */
  y: number;
  /** 关闭回调 */
  onClose: () => void;
  /** 展开全部 */
  onExpandAll?: () => void;
  /** 折叠全部 */
  onCollapseAll?: () => void;
  /** 复制路径 */
  onCopyPath?: () => void;
  /** 复制名称 */
  onCopyName?: () => void;
  /** 在时间线中查看 */
  onViewTimeline?: () => void;
  /** 提取文件 */
  onExtract?: () => void;
  /** 属性 */
  onProperties?: () => void;
  /** 节点名称 (用于复制) */
  nodeName?: string;
  /** 节点路径 (用于复制) */
  nodePath?: string;
}

export function TreeContextMenu({
  x,
  y,
  onClose,
  onExpandAll,
  onCollapseAll,
  onCopyPath,
  onCopyName,
  onViewTimeline,
  onExtract,
  onProperties,
  nodeName,
  nodePath,
}: TreeContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  // 点击外部关闭
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    // 延迟添加事件，避免立即触发
    const timer = setTimeout(() => {
      document.addEventListener('mousedown', handleClickOutside);
    }, 0);

    return () => {
      clearTimeout(timer);
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [onClose]);

  // ESC 关闭
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  // 边界检测 — 使用 ref 测量实际高度
  const menuWidth = 200;
  const adjustedX = Math.min(x, window.innerWidth - menuWidth - 10);
  const adjustedY = Math.min(y, window.innerHeight - (menuRef.current?.offsetHeight ?? 300) - 10);

  // 复制到剪贴板
  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // 降级方案
      const textarea = document.createElement('textarea');
      textarea.value = text;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
    }
  };

  const items: ContextMenuItem[] = [
    {
      label: '展开全部',
      icon: FolderOpen,
      action: () => onExpandAll?.(),
    },
    {
      label: '折叠全部',
      icon: FolderMinus,
      action: () => onCollapseAll?.(),
      divider: true,
    },
    {
      label: '复制路径',
      icon: Copy,
      shortcut: 'Ctrl+C',
      action: () => {
        if (nodePath) copyToClipboard(nodePath);
        onCopyPath?.();
      },
    },
    {
      label: '复制名称',
      icon: Copy,
      action: () => {
        if (nodeName) copyToClipboard(nodeName);
        onCopyName?.();
      },
      divider: true,
    },
    {
      label: '在时间线中查看',
      icon: Clock,
      action: () => onViewTimeline?.(),
    },
    {
      label: '提取文件',
      icon: Download,
      action: () => onExtract?.(),
      divider: true,
    },
    {
      label: '属性',
      icon: Info,
      action: () => onProperties?.(),
    },
  ];

  return (
    <div
      ref={menuRef}
      className="fixed z-50 min-w-[180px] bg-white border border-[#ddd] shadow-lg rounded py-1"
      style={{ left: adjustedX, top: adjustedY }}
    >
      {items.map((item, index) => (
        <div key={index}>
          {item.divider && <div className="border-t border-[#eee] my-1" />}
          <Button
            type="button"
            variant="forensicsGhost"
            size="menuItem"
            onClick={() => {
              if (!item.disabled) {
                item.action();
                onClose();
              }
            }}
            disabled={item.disabled}
            className={item.disabled ? 'text-[#ccc]' : 'text-[#333] hover:bg-[#f0f0f0]'}
          >
            <item.icon size={14} className={item.disabled ? 'text-[#ccc]' : 'text-[#666]'} />
            <span className="flex-1">{item.label}</span>
            {item.shortcut && (
              <span className="text-[10px] text-[#999]">{item.shortcut}</span>
            )}
          </Button>
        </div>
      ))}
    </div>
  );
}
