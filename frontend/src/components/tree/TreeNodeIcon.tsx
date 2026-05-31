/**
 * TreeNodeIcon - 树节点图标组件
 *
 * 根据文件类型显示对应的图标和颜色。
 */

import { getFileIcon } from '@/lib/file-icons';
import type { FileTreeNode } from '@/types/models';

interface TreeNodeIconProps {
  /** 节点数据 */
  node: Pick<FileTreeNode, 'name' | 'entryType' | 'status' | 'expanded'> & {
    deleted?: boolean;
  };
  /** 图标大小 */
  size?: number;
  /** 额外的 className */
  className?: string;
}

export function TreeNodeIcon({ node, size = 12, className = '' }: TreeNodeIconProps) {
  const iconInfo = getFileIcon(node);
  const IconComponent = iconInfo.icon;

  return (
    <IconComponent
      size={size}
      style={{ color: iconInfo.color }}
      className={`shrink-0 ${className}`}
    />
  );
}
