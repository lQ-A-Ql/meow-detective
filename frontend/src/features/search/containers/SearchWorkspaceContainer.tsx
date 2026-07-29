import { SearchWorkspace } from '@/features/search/components/SearchWorkspace';
import { useSearchWorkspaceModel } from '@/features/search/use-search-workspace-model';

export function SearchWorkspaceContainer() {
  const model = useSearchWorkspaceModel();
  return <SearchWorkspace model={model} />;
}
