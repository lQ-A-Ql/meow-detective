import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import type { ImageIssueSeverity, ImageMetadata, ImageMetadataWarning, ImageStructureFinding } from '@/types/viewer';

interface ImageMetadataDetailsProps {
  metadata?: ImageMetadata | null;
  fileSize?: number;
  metadataOnly?: boolean;
}

const exifFields = [
  ['orientation', 'orientation'],
  ['make', 'make'],
  ['model', 'model'],
  ['software', 'software'],
  ['dateTimeOriginal', 'dateTimeOriginal'],
  ['createDate', 'createDate'],
  ['modifyDate', 'modifyDate'],
  ['lensModel', 'lensModel'],
  ['latitude', 'latitude'],
  ['longitude', 'longitude'],
  ['altitude', 'altitude'],
  ['gpsDateTime', 'gpsDateTime'],
] as const;

function formatBytes(bytes?: number) {
  if (bytes === undefined || bytes === null) return '-';
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 || unitIndex === 0 ? Math.round(value) : value.toFixed(1)} ${units[unitIndex]}`;
}

function formatOffset(offset?: number) {
  return offset === undefined ? undefined : `0x${offset.toString(16)}`;
}

function severityClass(severity: ImageIssueSeverity) {
  if (severity === 'error') return 'text-forensics-error-text';
  if (severity === 'warning') return 'text-forensics-warning-text';
  return 'text-forensics-muted-light';
}

function IssueLine({
  code,
  severity,
  message,
  offset,
}: {
  code: string;
  severity: ImageIssueSeverity;
  message: string;
  offset?: number;
}) {
  const { t } = useTranslation();
  const text = t(`fileBrowser.preview.imageMetadata.warningCodes.${code}`, { defaultValue: message });
  return (
    <div className={`space-y-0.5 font-mono text-[10px] ${severityClass(severity)}`} data-severity={severity}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="uppercase">{severity}</span>
        <span>{code}</span>
        {offset !== undefined ? <span>{formatOffset(offset)}</span> : null}
      </div>
      <div className="break-all">{text}</div>
    </div>
  );
}

function WarningList({ warnings }: { warnings?: ImageMetadataWarning[] }) {
  const items = warnings ?? [];
  if (items.length === 0) return null;
  return (
    <div className="space-y-1">
      {items.map((warning, index) => (
        <IssueLine
          key={`${warning.code}-${warning.offset ?? index}`}
          code={warning.code}
          severity={warning.severity}
          message={warning.message}
          offset={warning.offset}
        />
      ))}
    </div>
  );
}

function FindingList({ findings }: { findings?: ImageStructureFinding[] }) {
  const items = findings ?? [];
  if (items.length === 0) return null;
  return (
    <div className="space-y-1">
      {items.map((finding, index) => (
        <IssueLine
          key={`${finding.code}-${finding.offset ?? index}`}
          code={finding.code}
          severity={finding.severity}
          message={finding.detail ?? finding.code}
          offset={finding.offset}
        />
      ))}
    </div>
  );
}

function HashRow({ label, value }: { label: string; value: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    if (!navigator.clipboard?.writeText) return;
    await navigator.clipboard.writeText(value);
    setCopied(true);
  };
  return (
    <div className="space-y-0.5">
      <div className="text-[10px] text-forensics-muted-light">{label}</div>
      <div className="flex items-start gap-1">
        <span className="break-all font-mono text-[10px] text-forensics-text-secondary">{value}</span>
        <Tooltip
          content={
            copied ? t('fileBrowser.preview.imageMetadata.copied') : t('fileBrowser.preview.imageMetadata.copyHash')
          }
        >
          <Button
            aria-label={
              copied ? t('fileBrowser.preview.imageMetadata.copied') : t('fileBrowser.preview.imageMetadata.copyHash')
            }
            type="button"
            size="xs"
            variant="forensicsOutline"
            onClick={() => {
              void copy();
            }}
          >
            <ActionIcon name="copy" />
          </Button>
        </Tooltip>
      </div>
    </div>
  );
}

export function ImageMetadataDetails({ metadata, fileSize, metadataOnly = false }: ImageMetadataDetailsProps) {
  const { t } = useTranslation();
  if (!metadata) return null;
  const structure = metadata.structure;
  const warnings = metadata.warnings ?? [];
  const exifBlocks = metadata.exifBlocks ?? [];
  const segments = structure?.segments ?? [];
  const findings = structure?.findings ?? [];
  const visibleExif = exifFields.filter(([key]) => {
    const value = metadata[key];
    return value !== undefined && value !== null && value !== '';
  });
  const dimensions =
    metadata.width !== undefined && metadata.height !== undefined ? `${metadata.width} × ${metadata.height}` : '-';
  const sectionLabel = (key: 'exif' | 'structure' | 'warnings' | 'hashes') => (
    <div className="mb-1 border-b border-forensics-border text-[10px] uppercase tracking-wide text-forensics-muted-light">
      {t(`fileBrowser.preview.imageMetadata.sections.${key}`)}
    </div>
  );

  return (
    <div className="space-y-2">
      {metadataOnly ? (
        <div className="space-y-0.5 border border-forensics-warning-border bg-forensics-warning-bg px-2 py-1 text-[10px] text-forensics-warning-text">
          <div className="font-light">{t('fileBrowser.preview.imageMetadata.metadataOnlyTitle')}</div>
          <div>{t('fileBrowser.preview.imageMetadata.metadataOnlyDetail')}</div>
        </div>
      ) : null}

      <div className="grid grid-cols-[92px_1fr] gap-1 font-mono text-[10px]">
        <div className="text-forensics-muted-light">{t('fileBrowser.preview.imageMetadata.status')}</div>
        <div className="text-forensics-text-secondary">
          {t(`fileBrowser.preview.imageMetadata.statuses.${metadata.status}`)}
        </div>
        <div className="text-forensics-muted-light">{t('fileBrowser.preview.imageMetadata.fields.format')}</div>
        <div className="break-all text-forensics-text-secondary">{metadata.format ?? '-'}</div>
        <div className="text-forensics-muted-light">{t('fileBrowser.preview.imageMetadata.fields.dimensions')}</div>
        <div className="text-forensics-text-secondary">{dimensions}</div>
        <div className="text-forensics-muted-light">{t('fileBrowser.preview.imageMetadata.fields.fileSize')}</div>
        <div className="text-forensics-text-secondary">{formatBytes(fileSize)}</div>
      </div>

      {warnings.length > 0 ? (
        <div>
          {sectionLabel('warnings')}
          <WarningList warnings={warnings} />
        </div>
      ) : null}

      {visibleExif.length > 0 || exifBlocks.length > 0 ? (
        <div className="space-y-1">
          {sectionLabel('exif')}
          {visibleExif.map(([key, label]) => (
            <div key={key} className="grid grid-cols-[92px_1fr] gap-2 font-mono text-[10px]">
              <span className="text-forensics-muted-light">
                {t(`fileBrowser.preview.imageMetadata.fields.${label}`)}
              </span>
              <span className="break-all text-forensics-text-secondary">{String(metadata[key])}</span>
            </div>
          ))}
          {exifBlocks.map((block, index) => (
            <details key={`${block.source}-${block.offset}-${index}`} className="font-mono text-[10px]">
              <summary className="cursor-pointer text-forensics-text-secondary">
                {block.source} · {formatOffset(block.offset)} · {block.length} B
              </summary>
              <div className="mt-1 space-y-1 border-l border-forensics-border pl-2">
                <div className="grid grid-cols-[92px_1fr] gap-2">
                  <span className="text-forensics-muted-light">
                    {t('fileBrowser.preview.imageMetadata.fields.offset')}
                  </span>
                  <span>{formatOffset(block.offset) ?? '-'}</span>
                  <span className="text-forensics-muted-light">
                    {t('fileBrowser.preview.imageMetadata.fields.length')}
                  </span>
                  <span>{block.length}</span>
                  <span className="text-forensics-muted-light">
                    {t('fileBrowser.preview.imageMetadata.fields.decoded')}
                  </span>
                  <span>{block.decoded ? 'true' : 'false'}</span>
                  <span className="text-forensics-muted-light">
                    {t('fileBrowser.preview.imageMetadata.fields.fieldCount')}
                  </span>
                  <span>{block.fieldCount}</span>
                </div>
                <WarningList warnings={block.warnings} />
              </div>
            </details>
          ))}
        </div>
      ) : null}

      {structure ? (
        <div className="space-y-1">
          {sectionLabel('structure')}
          <div className="grid grid-cols-[92px_1fr] gap-1 font-mono text-[10px]">
            <span className="text-forensics-muted-light">
              {t('fileBrowser.preview.imageMetadata.fields.declaredEnd')}
            </span>
            <span className="text-forensics-text-secondary">{formatOffset(structure.declaredEnd) ?? '-'}</span>
            <span className="text-forensics-muted-light">
              {t('fileBrowser.preview.imageMetadata.fields.trailingBytes')}
            </span>
            <span className="text-forensics-text-secondary">
              {structure.trailingBytes === undefined ? '-' : formatBytes(structure.trailingBytes)}
            </span>
            <span className="text-forensics-muted-light">
              {t('fileBrowser.preview.imageMetadata.fields.truncated')}
            </span>
            <span className="text-forensics-text-secondary">{structure.truncated ? 'true' : 'false'}</span>
            <span className="text-forensics-muted-light">
              {t('fileBrowser.preview.imageMetadata.fields.segmentCount')}
            </span>
            <span className="text-forensics-text-secondary">{structure.segmentCount}</span>
          </div>
          {segments.length > 0 ? (
            <div className="text-[10px] text-forensics-muted-light">
              {t('fileBrowser.preview.imageMetadata.fields.segments')}
            </div>
          ) : null}
          {segments.length > 0 ? (
            <div className="max-h-40 space-y-0.5 overflow-auto">
              {segments.map((segment, index) => (
                <div
                  key={`${segment.kind}-${segment.offset}-${index}`}
                  className="grid grid-cols-[72px_64px_1fr] gap-1 font-mono text-[10px] text-forensics-text-secondary"
                >
                  <span className="truncate">{segment.kind}</span>
                  <span>{formatOffset(segment.offset)}</span>
                  <span className="break-all">
                    {segment.length} B{segment.detail ? ` · ${segment.detail}` : ''}
                  </span>
                </div>
              ))}
            </div>
          ) : null}
          {structure.segmentsTruncated ? (
            <div className="text-[10px] text-forensics-muted">
              {t('fileBrowser.preview.imageMetadata.truncatedList')}
            </div>
          ) : null}
          {findings.length > 0 ? (
            <div className="text-[10px] text-forensics-muted-light">
              {t('fileBrowser.preview.imageMetadata.fields.findings')}
            </div>
          ) : null}
          <FindingList findings={findings} />
          {structure.hashes.wholeFileSha256 || structure.hashes.trailingSha256 ? (
            <div className="space-y-1">
              {sectionLabel('hashes')}
              {structure.hashes.wholeFileSha256 ? (
                <HashRow
                  label={t('fileBrowser.preview.imageMetadata.hashLabels.wholeFile')}
                  value={structure.hashes.wholeFileSha256}
                />
              ) : null}
              {structure.hashes.trailingSha256 ? (
                <HashRow
                  label={t('fileBrowser.preview.imageMetadata.hashLabels.trailing')}
                  value={structure.hashes.trailingSha256}
                />
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
