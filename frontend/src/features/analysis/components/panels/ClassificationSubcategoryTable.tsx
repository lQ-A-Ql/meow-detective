import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { DenseColumn } from '@/components/tables/DenseDataTable';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { ClassifiedFileRow, ClassificationSubcategory } from '@/types/models';
import { formatSize } from './helpers';

type Translate = (key: string, options?: Record<string, unknown>) => string;

// Column titles are translated, so the array is built from `t` and memoized in
// the component; the stable reference keeps DenseDataTable row memoization intact.
function classificationColumns(t: Translate): DenseColumn<ClassifiedFileRow>[] {
  return [
  {
    key: 'name',
    title: t('analysisPanels.subcategory.fileName'),
    className: 'min-w-[220px]',
    render: (row) => row.name,
  },
  {
    key: 'path',
    title: t('search.columns.path'),
    className: 'min-w-[260px]',
    render: (row) => (
      <span className="font-mono text-[10px] text-forensics-muted-light">{row.path}</span>
    ),
  },
  {
    key: 'magicType',
    title: t('analysisPanels.subcategory.magicType'),
    className: 'w-[90px]',
    render: (row) => row.magicType ?? '-',
  },
  {
    key: 'source',
    title: t('analysisPanels.subcategory.source'),
    className: 'w-[70px]',
    render: (row) => (
      <span
        className={
          row.classificationSource === 'magic'
            ? 'text-forensics-success-text'
            : 'text-forensics-muted'
        }
      >
        {row.classificationSource === 'magic' ? t('analysisPanels.subcategory.magic') : t('analysisPanels.subcategory.inferred')}
      </span>
    ),
  },
  {
    key: 'size',
    title: t('common.sort.size'),
    className: 'w-[100px] text-right',
    render: (row) => formatSize(row.size),
  },
  ];
}

export function ClassificationSubcategoryTable({
  subcategory,
  selectedFileId,
  onSelect,
}: {
  subcategory: ClassificationSubcategory;
  selectedFileId?: string;
  onSelect: (row: ClassifiedFileRow) => void;
}) {
  const { t } = useTranslation();
  const columns = useMemo(() => classificationColumns(t), [t]);
  return (
    <div>
      <div className="mb-1 flex items-center gap-2 text-[11px]">
        <span className="rounded-none bg-forensics-info-bg px-1.5 py-0.5 text-forensics-text">
          {subcategory.name}
        </span>
        <span className="text-forensics-muted-lighter">
          {t('analysisPanels.countWithSize', { count: subcategory.fileCount, size: formatSize(subcategory.totalSize) })}
          {subcategory.truncated
            ? ` · ${t('analysisPanels.subcategory.sampled', { count: subcategory.files.length })}`
            : ''}
        </span>
      </div>
      <DenseDataTableFrame rowCount={subcategory.files.length}>
        <DenseDataTable
          rows={subcategory.files}
          columns={columns}
          getRowKey={(row) => row.fileId}
          selectedRowKey={selectedFileId}
          onRowClick={onSelect}
          emptyTitle={t('analysisPanels.subcategory.emptyTitle')}
          emptyDescription={t('analysisPanels.subcategory.emptyDescription')}
        />
      </DenseDataTableFrame>
    </div>
  );
}
