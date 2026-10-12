import { TextViewerController } from '@/features/files/containers/TextViewerController';
import type { TextViewerProps } from '@/features/files/text-viewer/types';

export type { TextViewerProps };

/** Stable viewer entry; search state and viewport orchestration live in the controller. */
export function TextViewer(props: TextViewerProps) {
  return <TextViewerController {...props} />;
}
