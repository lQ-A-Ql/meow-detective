import { AlertCircle, LoaderCircle, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import type { WeChatLoadState } from '@/features/analysis/wechat/types';

export function WeChatLoadFooter({ state }: { state: WeChatLoadState }) {
  const { t } = useTranslation();
  if (state.total === 0 && !state.failed) return null;
  return (
    <div className="flex min-h-9 items-center justify-between gap-3 border-t border-forensics-border bg-forensics-panel px-3 py-1.5 text-[11px]">
      <span className="font-mono text-forensics-muted">
        {t('wechatWorkspace.loaded', { loaded: state.loaded, total: state.total })}
      </span>
      {state.failed ? (
        <Button variant="forensicsGhost" size="xs" onClick={state.retry}>
          <AlertCircle />
          {t('wechatWorkspace.retry')}
        </Button>
      ) : state.hasMore ? (
        <Button
          variant="forensicsGhost"
          size="xs"
          disabled={state.loadingMore}
          onClick={state.loadMore}
        >
          {state.loadingMore ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
          {t(state.loadingMore ? 'wechatWorkspace.loadingMore' : 'wechatWorkspace.loadMore')}
        </Button>
      ) : null}
    </div>
  );
}
