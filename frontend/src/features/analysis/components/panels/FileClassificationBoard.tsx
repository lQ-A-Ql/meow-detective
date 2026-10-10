import { useTranslation } from 'react-i18next';
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
import { TokenIcon } from '@/components/data-display';

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
  const { t } = useTranslation();
  if (!board) {
    return <EmptyLine text={t('analysisPanels.board.unavailable')} />;
  }

  return (
    <div className="space-y-6">
      <SummaryStrip
        items={[
          [t('analysisPanels.board.totalFiles'), board.totalFiles.toString()],
          [t('analysisPanels.board.totalSize'), formatSize(board.totalSize)],
          [t('analysisPanels.board.magicClassified'), board.magicClassifiedCount.toString()],
          [t('analysisPanels.board.metadataClassified'), board.metadataClassifiedCount.toString()],
          [t('analysisPanels.board.groupCount'), board.groups.length.toString()],
        ]}
      />

      {board.warnings?.length ? <WarningList warnings={board.warnings} /> : null}

      {board.groups.length === 0 ? (
        <EmptyLine text={t('analysisPanels.board.empty')} />
      ) : (
        <div className="space-y-5">
          {board.groups.map((group) => {
            const iconKey = FAMILY_KEY[group.category] ?? 'Other';
            const Icon = CATEGORY_ICONS[iconKey] ?? CATEGORY_ICONS.Other;
            const color = CATEGORY_COLORS[iconKey] ?? CATEGORY_COLORS.Other;
            return (
              <CollapsibleSection
                key={group.category}
                className="rounded-none bg-transparent p-4"
                contentClassName="mt-3 space-y-3"
                title={
                  <>
                    <TokenIcon icon={Icon} size={17} color={color} />
                    <h4 className="text-[13px] font-light text-forensics-text">{group.displayName}</h4>
                    <span className="text-[11px] text-forensics-muted-lighter">
                      {t('analysisPanels.countWithSize', { count: group.fileCount, size: formatSize(group.totalSize) })}
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
