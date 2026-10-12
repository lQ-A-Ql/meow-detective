import type { ReactElement, ReactNode } from 'react';

export interface DenseColumn<T> {
  key: string;
  title: ReactNode;
  className?: string;
  /** 是否可排序 */
  sortable?: boolean;
  /** 排序键 (用于回调) */
  sortKey?: string;
  /** 搜索/列筛选用的文本提取；不提供则该列不参与匹配。 */
  text?: (row: T) => string;
  /** 该列出现在工具条的列筛选下拉中（需要同时提供 text）。 */
  filterable?: boolean;
  render: (row: T) => ReactNode;
}

export interface DenseDataTableProps<T> {
  columns: DenseColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  selectedRowKey?: string;
  onRowClick?: (row: T) => void;
  onRowDoubleClick?: (row: T) => void;
  renderRowContextMenu?: (row: T, trigger: ReactElement) => ReactElement;
  emptyTitle?: string;
  emptyDescription?: string;
  /** Optional next step rendered inside the empty state. */
  emptyAction?: ReactNode;
  /** 当前排序键 */
  sortKey?: string;
  /** 排序方向 */
  sortDirection?: 'asc' | 'desc';
  /** 排序回调 */
  onSort?: (key: string) => void;
  /** 接近当前数据末尾时请求下一段。 */
  onReachEnd?: () => void;
  /** 逻辑数据上下文变化时重置滚动位置与续载去重状态。 */
  loadContextKey?: string;
  /** 查询缓存完成一次更新时变化，用于解除被取消请求留下的续载锁。 */
  loadStateKey?: string | number;
  hasMore?: boolean;
  loadingMore?: boolean;
  loadMoreFailed?: boolean;
  loadMoreErrorText?: string;
  retryLoadMoreLabel?: string;
  /** 续载失败后的恢复动作；游标分页通常应重建查询链，而非重放旧游标。 */
  onRetryLoadMore?: () => unknown;
  initialLoadFailed?: boolean;
  initialLoadErrorText?: string;
  retryInitialLoadLabel?: string;
  onRetryInitialLoad?: () => void;
  /** 为动态多列表格保留最小列宽，并允许整个表格横向滚动。 */
  horizontalScroll?: boolean;
  minColumnWidth?: number;
  /** 开启客户端搜索/列筛选工具条；过滤只作用于当前已加载行。 */
  filterable?: boolean;
}
