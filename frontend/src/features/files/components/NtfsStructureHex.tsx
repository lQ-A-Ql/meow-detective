import { useTranslation } from 'react-i18next';
import { HexViewer } from '@/components/viewers/HexViewer';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { KeyValueField } from '@/components/data-display';
import type { NtfsRecordSourceRange } from '@/types/files';

export interface NtfsStructureHexProps {
  hex?: { bytes: number[]; offset: number; length: number; decoded: boolean };
  sourceRanges: NtfsRecordSourceRange[];
  onDecodedChange?: (decoded: boolean) => void;
}

export function NtfsStructureHex({ hex, sourceRanges, onDecodedChange }: NtfsStructureHexProps) {
  const { t } = useTranslation();
  if (!hex) return null;
  const label = t(
    hex.decoded ? 'fileBrowser.inspector.ntfs.showSourceBytes' : 'fileBrowser.inspector.ntfs.showDecodedBytes',
  );
  return (
    <div className="space-y-2">
      <div>
        {t(
          hex.decoded
            ? 'fileBrowser.inspector.ntfs.decodedRecordBytes'
            : 'fileBrowser.inspector.ntfs.sourceRecordBytes',
        )}
      </div>
      <Tooltip content={label}>
        <Button
          aria-label={label}
          type="button"
          size="xs"
          variant="forensicsOutline"
          onClick={() => onDecodedChange?.(!hex.decoded)}
        >
          <ActionIcon name={hex.decoded ? 'eye-off' : 'eye'} />
        </Button>
      </Tooltip>
      {sourceRanges.map((range) => (
        <KeyValueField
          key={range.recordOffset}
          label={t('fileBrowser.inspector.ntfs.sourceRange', { offset: range.recordOffset, length: range.length })}
          value={range.sourceOffset}
        />
      ))}
      <div className="h-56 overflow-auto">
        <HexViewer
          lines={[]}
          rawBytes={hex.bytes}
          fileSize={hex.bytes.length}
          activeOffset={hex.offset}
          highlightRange={{ start: hex.offset, end: hex.offset + hex.length - 1 }}
        />
      </div>
    </div>
  );
}
