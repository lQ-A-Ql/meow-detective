import { useVirtualListModel } from '@/hooks/use-virtual-list-model';
import { VirtualListView } from './VirtualListView';
import type { VirtualListProps } from './virtual-list-types';
export function VirtualList<T>(props: VirtualListProps<T>) { return <VirtualListView model={useVirtualListModel(props)} />; }
