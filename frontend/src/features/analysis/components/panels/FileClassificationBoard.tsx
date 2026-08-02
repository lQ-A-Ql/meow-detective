import { CollapsibleSection } from '@/components/layout/CollapsibleSection';
import { ClassificationSubcategoryTable } from './ClassificationSubcategoryTable';
import type {
  ClassifiedFileRow,
  FileClassificationBoard as FileClassificationBoardData,
} from '@/types/models';
import {
  CATEGORY_COLORS,
  CATEGORY_ICONS,
  EmptyLine,
  formatSize,
  StatusPill,
  SummaryStrip,
  WarningList,
} from './helpers';

const FAMILY_KEY: Record<string, string> = {
  documents: 'Documents',
  images: 'Images',
  media: 'Media',
  databases: 'Databases',
  executables: 'Executables',
  archives: 'Archives',
  system: 'System',
  forensics: 'Forensics',
  other: 'Other',
};

export function FileClassificationBoard({
  board,
  selectedFileId,
  onSelect,
}: {
  board?: FileClassificationBoardData;
  selectedFileId?: string;
  onSelect: (row: ClassifiedFileRow) => void;
}) {
  if (!board) {
    return <EmptyLine text="文件分类数据暂不可用。" />;
  }

  return (
    <div className="space-y-6">
      <SummaryStrip
        items={[
          ['文件总数', board.totalFiles.toString()],
          ['文件总大小', formatSize(board.totalSize)],
          ['魔数识别', board.magicClassifiedCount.toString()],
          ['元数据推断', board.metadataClassifiedCount.toString()],
          ['分类族', board.groups.length.toString()],
        ]}
      />

      {board.warnings?.length ? <WarningList warnings={board.warnings} /> : null}

      {board.groups.length === 0 ? (
        <EmptyLine text="未发现可分类文件。" />
      ) : (
        <div className="space-y-5">
          {board.groups.map((group) => {
            const iconKey = FAMILY_KEY[group.category] ?? 'Other';
            const Icon = CATEGORY_ICONS[iconKey] ?? CATEGORY_ICONS.Other;
            const color = CATEGORY_COLORS[iconKey] ?? CATEGORY_COLORS.Other;
            return (
              <CollapsibleSection
                key={group.category}
                className="rounded-none border border-forensics-border bg-forensics-surface p-4"
                contentClassName="mt-3 space-y-3"
                title={
                  <>
                    <Icon size={17} style={{ color }} />
                    <h4 className="text-[13px] font-light text-forensics-text">{group.displayName}</h4>
                    <span className="text-[11px] text-forensics-muted-lighter">
                      {group.fileCount} 个 · {formatSize(group.totalSize)}
                    </span>
                  </>
                }
                headerExtra={<StatusPill status={board.status} />}
              >
                {group.subcategories.map((sub) => (
                  <ClassificationSubcategoryTable
                    key={sub.name}
                    subcategory={sub}
                    selectedFileId={selectedFileId}
                    onSelect={onSelect}
                  />
                ))}
              </CollapsibleSection>
            );
          })}
        </div>
      )}
    </div>
  );
}
