import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/app/components/ui/dialog';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { getTextPreview } from '@/lib/api/files';
import type { LinuxSystemConfig } from '@/types/models';

export function LinuxServicesPanel({ rows }: { rows: LinuxSystemConfig[] }) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState<LinuxSystemConfig>();
  const preview = useQuery({
    queryKey: ['linux-service-preview', selected?.fileId ?? null],
    queryFn: () => getTextPreview(selected?.fileId ?? '', 1024 * 1024),
    enabled: Boolean(selected?.fileId),
    retry: false,
  });
  const services = rows.filter((row) => row.configKind === 'systemdServiceUnit');
  const configured = services.length;
  const running = services.filter((row) => ['active', 'running'].includes((row.state ?? '').toLowerCase())).length;
  const columns: DenseColumn<LinuxSystemConfig>[] = [
    { key: 'name', title: t('linuxArtifacts.services.name'), render: (row) => row.name ?? row.line, text: (row) => row.name ?? row.line },
    { key: 'state', title: t('linuxArtifacts.services.state'), render: (row) => row.state ?? '-', text: (row) => row.state ?? '' },
    { key: 'enabled', title: t('linuxArtifacts.services.enabled'), render: (row) => row.enabled ? t('linuxArtifacts.values.yes') : t('linuxArtifacts.values.no'), text: (row) => row.enabled ? 'yes' : 'no' },
    { key: 'unitFile', title: t('linuxArtifacts.services.unitFile'), className: 'min-w-[260px]', render: (row) => row.unitFile ?? row.sourcePath, text: (row) => row.unitFile ?? row.sourcePath },
    { key: 'preview', title: t('linuxArtifacts.services.preview'), render: (row) => <Button type="button" variant="forensicsLink" size="xs" onClick={() => setSelected(row)}>{t('linuxArtifacts.services.open')}</Button>, text: () => t('linuxArtifacts.services.open') },
  ];
  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-3"><div className="text-[10px] text-forensics-muted">{t('linuxArtifacts.services.configured')}</div><div className="mt-1 font-mono text-lg">{configured}</div></div>
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-3"><div className="text-[10px] text-forensics-muted">{t('linuxArtifacts.services.running')}</div><div className="mt-1 font-mono text-lg">{running}</div></div>
      </div>
      <DenseDataTableFrame rowCount={services.length}>
        <DenseDataTable rows={services} columns={columns} getRowKey={(row) => row.artifactId} filterable emptyTitle={t('linuxArtifacts.services.empty')} emptyDescription={t('linuxArtifacts.services.emptyDescription')} />
      </DenseDataTableFrame>
      <Dialog open={Boolean(selected)} onOpenChange={(open) => { if (!open) setSelected(undefined); }}>
        <DialogContent className="max-h-[80vh] max-w-4xl overflow-hidden">
          <DialogHeader>
            <DialogTitle>{selected?.name ?? selected?.line ?? t('linuxArtifacts.services.preview')}</DialogTitle>
            <DialogDescription className="font-mono">{selected?.sourcePath ?? ''}</DialogDescription>
          </DialogHeader>
          <pre className="max-h-[60vh] overflow-auto whitespace-pre-wrap rounded-none border border-forensics-border bg-forensics-ink p-4 font-mono text-[11px] text-forensics-paper">{preview.isLoading ? t('linuxArtifacts.services.loading') : preview.error ? t('linuxArtifacts.services.failed') : preview.data?.content ?? t('linuxArtifacts.services.emptyPreview')}</pre>
        </DialogContent>
      </Dialog>
    </div>
  );
}
