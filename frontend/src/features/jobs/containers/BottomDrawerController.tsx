import { useResizableHeight } from '@/hooks/use-resizable-height';
import { BottomDrawerView } from '../components/BottomDrawerView';
import type { BottomDrawerProps } from '../components/BottomDrawerView';

export function BottomDrawerController(props: BottomDrawerProps) {
  const resize = useResizableHeight({
    defaultHeight: 224,
    minHeight: 128,
    maxHeight: 600,
    storageKey: 'bottomDrawerHeight',
  });
  return <BottomDrawerView {...props} resize={resize} />;
}
