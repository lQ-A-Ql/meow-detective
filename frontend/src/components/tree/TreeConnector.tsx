/**
 * TreeConnector - 层级连接线组件
 *
 * 为文件树提供可视化的层级连接线，类似 VS Code / Explorer 的效果。
 */

interface TreeConnectorProps {
  /** 当前节点深度 (0 = 根节点) */
  depth: number;
  /** 是否是父节点的最后一个子节点 */
  isLast: boolean;
}

export function TreeConnector({ depth, isLast }: TreeConnectorProps) {
  if (depth <= 0) {
    return null;
  }

  return (
    <span className="inline-flex shrink-0" aria-hidden="true">
      {/* 每层的竖线 */}
      {Array.from({ length: depth - 1 }, (_, i) => (
        <span
          key={i}
          className="inline-block w-4 border-l border-forensics-border-strong"
        />
      ))}
      {/* 当前层级的连接线 */}
      <span
        className="inline-block w-4 relative"
        style={{ height: '24px' }}
      >
        {/* 横线 */}
        <span className="absolute top-0 left-0 w-4 border-b border-forensics-border-strong" style={{ top: '12px' }} />
        {/* 竖线 (如果不是最后一个节点，竖线延伸到底部) */}
        {!isLast && (
          <span className="absolute top-0 left-0 h-full border-l border-forensics-border-strong" />
        )}
        {/* 竖线 (如果是最后一个节点，竖线只到中间) */}
        {isLast && (
          <span className="absolute top-0 left-0 border-l border-forensics-border-strong" style={{ height: '12px' }} />
        )}
      </span>
    </span>
  );
}
