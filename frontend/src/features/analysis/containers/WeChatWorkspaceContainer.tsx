import { WeChatWorkspace } from '@/features/analysis/components/panels/wechat/WeChatWorkspace';
import { useWeChatWorkspaceModel } from '@/features/analysis/wechat/use-wechat-workspace-model';

export function WeChatWorkspaceContainer({ dataSourceId }: { dataSourceId: string }) {
  const model = useWeChatWorkspaceModel(dataSourceId);
  return <WeChatWorkspace model={model} />;
}
