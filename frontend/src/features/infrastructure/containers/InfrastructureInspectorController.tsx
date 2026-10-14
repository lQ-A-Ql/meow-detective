import type { ComponentProps } from 'react';
import { InfrastructureInspector } from '../components/InfrastructureInspector';
import { infrastructureInspectorModel } from '../logic/inspector-model';
import { useCopyText } from '@/hooks/use-copy-text';

export function InfrastructureInspectorController(props: Omit<ComponentProps<typeof InfrastructureInspector>, 'presentation' | 'onCopyId'>) {
  const presentation = infrastructureInspectorModel(props.node, props.edges);
  const copy = useCopyText(props.node.id);
  return <InfrastructureInspector {...props} presentation={presentation} onCopyId={copy} />;
}
