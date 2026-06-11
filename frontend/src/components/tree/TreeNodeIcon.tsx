/**
 * TreeNodeIcon - 树节点图标组件
 *
 * 根据文件类型显示对应的图标和颜色。
 */

import { FileIconWithStatusOverlay } from '@/components/files/FileIconWithStatusOverlay';
import type { FileTreeNode } from '@/types/models';

interface TreeNodeIconProps {
  /** 节点数据 */
  node: Pick<FileTreeNode, 'name' | 'entryType' | 'status' | 'expanded'> & {
    deleted?: boolean;
    hidden?: boolean;
    system?: boolean;
  };
  /** 图标大小 */
  size?: number;
  /** 额外的 className */
  className?: string;
}

export function TreeNodeIcon({ node, size = 12, className = '' }: TreeNodeIconProps) {
  return (
    <FileIconWithStatusOverlay
      name={node.name}
      entryType={node.entryType}
      status={node.status}
      expanded={node.expanded}
      deleted={node.deleted}
      hidden={node.hidden}
      system={node.system}
      size={size}
      className={className}
    />
  );
}
