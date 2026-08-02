import type { DenseColumn } from '@/components/tables/DenseDataTable';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { ClassifiedFileRow, ClassificationSubcategory } from '@/types/models';
import { formatSize } from './helpers';

const CLASSIFICATION_COLUMNS: DenseColumn<ClassifiedFileRow>[] = [
  {
    key: 'name',
    title: '文件名',
    className: 'min-w-[220px]',
    render: (row) => row.name,
  },
  {
    key: 'path',
    title: '路径',
    className: 'min-w-[260px]',
    render: (row) => (
      <span className="font-mono text-[10px] text-forensics-muted-light">{row.path}</span>
    ),
  },
  {
    key: 'magicType',
    title: '魔数类型',
    className: 'w-[90px]',
    render: (row) => row.magicType ?? '-',
  },
  {
    key: 'source',
    title: '判定',
    className: 'w-[70px]',
    render: (row) => (
      <span
        className={
          row.classificationSource === 'magic'
            ? 'text-forensics-success-text'
            : 'text-forensics-muted'
        }
      >
        {row.classificationSource === 'magic' ? '魔数' : '推断'}
      </span>
    ),
  },
  {
    key: 'size',
    title: '大小',
    className: 'w-[100px] text-right',
    render: (row) => formatSize(row.size),
  },
];

export function ClassificationSubcategoryTable({
  subcategory,
  selectedFileId,
  onSelect,
}: {
  subcategory: ClassificationSubcategory;
  selectedFileId?: string;
  onSelect: (row: ClassifiedFileRow) => void;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center gap-2 text-[11px]">
        <span className="rounded-none bg-forensics-info-bg px-1.5 py-0.5 text-forensics-text">
          {subcategory.name}
        </span>
        <span className="text-forensics-muted-lighter">
          {subcategory.fileCount} 个 · {formatSize(subcategory.totalSize)}
          {subcategory.truncated ? ` · 抽样 ${subcategory.files.length} 个` : ''}
        </span>
      </div>
      <DenseDataTableFrame rowCount={subcategory.files.length}>
        <DenseDataTable
          rows={subcategory.files}
          columns={CLASSIFICATION_COLUMNS}
          getRowKey={(row) => row.fileId}
          selectedRowKey={selectedFileId}
          onRowClick={onSelect}
          emptyTitle="暂无文件"
          emptyDescription="该子分类当前没有可展示的抽样文件。"
        />
      </DenseDataTableFrame>
    </div>
  );
}
