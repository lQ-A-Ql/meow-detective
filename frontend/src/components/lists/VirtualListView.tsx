import { ScrollArea } from '@/app/components/ui/scroll-area';
import { cn } from '@/app/components/ui/utils';
import type { useVirtualListModel } from '@/hooks/use-virtual-list-model';
export function VirtualListView<T>({ model }: { model: ReturnType<typeof useVirtualListModel<T>> }) {
  const { props } = model;
  return <ScrollArea className={cn('min-h-0 overflow-hidden', props.className)} style={props.style} viewportRef={model.setViewport} viewportClassName={props.viewportClassName} viewportProps={{ role: props.treeContent ? 'presentation' : 'list', 'aria-label': props.ariaLabel, tabIndex: props.viewportTabIndex ?? 0 }}>
    <div className="relative w-full" style={{ height: model.totalSize }}>
      {model.rows.map((row) => <div key={row.key} data-index={row.index} ref={model.measure} role={props.treeContent ? 'presentation' : 'listitem'} aria-posinset={props.treeContent ? undefined : row.index + 1} aria-setsize={props.treeContent ? undefined : props.items.length} className="absolute left-0 top-0 w-full" style={{ transform: `translateY(${row.start}px)`, height: props.fixedSize ? row.size : undefined }}>{props.renderItem(props.items[row.index], row.index)}</div>)}
    </div>
  </ScrollArea>;
}
