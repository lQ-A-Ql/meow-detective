import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { LinuxSystemConfig } from '@/types/models';

export function LinuxDockerPanel({ rows }: { rows: LinuxSystemConfig[] }) {
  const { t } = useTranslation();
  const containers = useMemo(() => rows.filter((row) => row.configKind === 'dockerContainer'), [rows]);
  const images = useMemo(() => rows.filter((row) => row.configKind === 'dockerImage'), [rows]);
  const containerColumns: DenseColumn<LinuxSystemConfig>[] = [
    { key: 'name', title: t('linuxArtifacts.docker.container'), render: (row) => row.name ?? row.line, text: (row) => row.name ?? row.line },
    { key: 'image', title: t('linuxArtifacts.docker.image'), render: (row) => row.value ?? '-', text: (row) => row.value ?? '' },
    { key: 'state', title: t('linuxArtifacts.docker.state'), render: (row) => row.state ?? '-', text: (row) => row.state ?? '' },
    { key: 'source', title: t('linuxArtifacts.columns.sourcePath'), render: (row) => row.sourcePath, text: (row) => row.sourcePath },
  ];
  const imageColumns: DenseColumn<LinuxSystemConfig>[] = [
    { key: 'repository', title: t('linuxArtifacts.docker.repository'), render: (row) => row.name ?? row.line, text: (row) => row.name ?? row.line },
    { key: 'tag', title: t('linuxArtifacts.docker.tag'), render: (row) => row.key ?? '-', text: (row) => row.key ?? '' },
    { key: 'digest', title: t('linuxArtifacts.docker.digest'), className: 'min-w-[260px]', render: (row) => row.value ?? '-', text: (row) => row.value ?? '' },
  ];
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-3"><div className="text-[10px] text-forensics-muted">{t('linuxArtifacts.docker.containers')}</div><div className="mt-1 font-mono text-lg">{containers.length}</div></div>
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-3"><div className="text-[10px] text-forensics-muted">{t('linuxArtifacts.docker.images')}</div><div className="mt-1 font-mono text-lg">{images.length}</div></div>
      </div>
      <section className="space-y-2"><h3 className="font-serif text-sm">{t('linuxArtifacts.docker.containers')}</h3><DenseDataTableFrame rowCount={containers.length}><DenseDataTable rows={containers} columns={containerColumns} getRowKey={(row) => row.artifactId} filterable emptyTitle={t('linuxArtifacts.docker.empty')} emptyDescription={t('linuxArtifacts.docker.emptyDescription')} /></DenseDataTableFrame></section>
      <section className="space-y-2"><h3 className="font-serif text-sm">{t('linuxArtifacts.docker.images')}</h3><DenseDataTableFrame rowCount={images.length}><DenseDataTable rows={images} columns={imageColumns} getRowKey={(row) => row.artifactId} filterable emptyTitle={t('linuxArtifacts.docker.emptyImages')} emptyDescription={t('linuxArtifacts.docker.emptyDescription')} /></DenseDataTableFrame></section>
    </div>
  );
}
