import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';

interface PluginWorkspaceViewProps {
  running: boolean;
  error?: string;
  onRun: () => void;
  children?: ReactNode;
}

export function PluginWorkspaceView({ running, error, onRun, children }: PluginWorkspaceViewProps) {
  const { t } = useTranslation();
  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3 border-b border-forensics-divider pb-3">
        <span className="text-[12px] text-forensics-text">{t('pluginModule.groupTitle')}</span>
        <Tooltip content={t('pluginModule.actions.runEnabled')}>
          <Button
            type="button"
            variant="forensicsOutline"
            size="xs"
            disabled={running}
            onClick={onRun}
            aria-label={t('pluginModule.actions.runEnabled')}
          >
            <ActionIcon name="play" />
            {t(running ? 'pluginModule.actions.running' : 'pluginModule.actions.runEnabled')}
          </Button>
        </Tooltip>
      </div>
      {error ? <p className="text-[11px] text-forensics-error-text">{error}</p> : null}
      {children}
    </section>
  );
}
