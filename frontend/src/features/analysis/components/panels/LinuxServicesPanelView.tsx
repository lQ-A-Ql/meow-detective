import { useTranslation } from 'react-i18next';
import { FileText } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { LinuxSystemConfig } from '@/types/models';
import type { LinuxServicesPanelModel } from '../../use-linux-services-panel-model';

export function LinuxServicesPanelView({ model }: { model: LinuxServicesPanelModel }) {
  const { t } = useTranslation();
  const columns: DenseColumn<LinuxSystemConfig>[] = [
    { key: 'name', title: t('linuxArtifacts.services.name'), render: (row) => row.name ?? row.line, text: (row) => row.name ?? row.line },
    { key: 'state', title: t('linuxArtifacts.services.state'), render: (row) => row.state ?? '-', text: (row) => row.state ?? '' },
    { key: 'enabled', title: t('linuxArtifacts.services.enabled'), render: (row) => row.enabled === undefined ? '-' : t(row.enabled ? 'linuxArtifacts.values.yes' : 'linuxArtifacts.values.no'), text: (row) => row.enabled === undefined ? '' : t(row.enabled ? 'linuxArtifacts.values.yes' : 'linuxArtifacts.values.no') },
    { key: 'unitFile', title: t('linuxArtifacts.services.unitFile'), className: 'min-w-[260px]', render: (row) => row.unitFile ?? row.sourcePath, text: (row) => row.unitFile ?? row.sourcePath },
    {
      key: 'preview',
      title: t('linuxArtifacts.services.preview'),
      render: (row) => (
        <Tooltip content={t('linuxArtifacts.services.open')}>
          <Button type="button" variant="forensicsGhost" size="xs" aria-label={t('linuxArtifacts.services.open')} disabled={!row.fileId} onClick={() => model.selectService(row)}>
            <FileText />
          </Button>
        </Tooltip>
      ),
      text: () => t('linuxArtifacts.services.open'),
    },
  ];
  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-3"><div className="text-[10px] text-forensics-muted">{t('linuxArtifacts.services.configured')}</div><div className="mt-1 font-mono text-lg">{model.configured}</div></div>
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-3"><div className="text-[10px] text-forensics-muted">{t('linuxArtifacts.services.enabledCount')}</div><div className="mt-1 font-mono text-lg">{model.enabled}</div></div>
      </div>
      <DenseDataTableFrame rowCount={model.services.length}>
        <DenseDataTable
          rows={model.services}
          columns={columns}
          getRowKey={(row) => row.artifactId}
          filterable
          emptyTitle={t('linuxArtifacts.services.empty')}
          emptyDescription={t('linuxArtifacts.services.emptyDescription')}
          loadContextKey={model.loadContextKey}
          loadStateKey={model.loadStateKey}
          hasMore={model.canLoadMore}
          loadingMore={model.loadingMore}
          loadMoreFailed={model.loadMoreFailed}
          onReachEnd={model.onLoadMore}
          onRetryLoadMore={model.onRetryLoadMore}
        />
      </DenseDataTableFrame>
      <Dialog open={Boolean(model.selected)} onOpenChange={(open) => { if (!open) model.closePreview(); }}>
        <DialogContent className="max-h-[80vh] max-w-4xl overflow-hidden">
          <DialogHeader>
            <DialogTitle>{model.selected?.name ?? model.selected?.line ?? t('linuxArtifacts.services.preview')}</DialogTitle>
            <DialogDescription className="font-mono">{model.selected?.sourcePath ?? ''}</DialogDescription>
          </DialogHeader>
          <pre className="max-h-[60vh] overflow-auto whitespace-pre-wrap rounded-none border border-forensics-border bg-forensics-ink p-4 font-mono text-[11px] text-forensics-paper">{model.previewLoading ? t('linuxArtifacts.services.loading') : model.previewFailed ? t('linuxArtifacts.services.failed') : model.previewContent ?? t('linuxArtifacts.services.emptyPreview')}</pre>
        </DialogContent>
      </Dialog>
    </div>
  );
}
