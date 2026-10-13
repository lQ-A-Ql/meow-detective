import { RegistryBrowserController, type RegistryBrowserProps } from './RegistryBrowserController';

/** Source changes remount controller state; query keys and session checks also bind the source. */
export function RegistryBrowserContainer(props: RegistryBrowserProps) {
  const { caseId, dataSourceId, sourceRevision } = props.context;
  return <RegistryBrowserController key={`${caseId}:${dataSourceId}:${sourceRevision}`} {...props} />;
}
