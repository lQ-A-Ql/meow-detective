import {
  Download,
  Eye,
  LoaderCircle,
  RefreshCw,
  Search,
  ScanSearch,
  X,
} from 'lucide-react';
import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { EmptyState, KeyValueField, SectionHeader } from '@/components/data-display';
import { DenseDataTable, type DenseColumn } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { HexViewer } from '@/components/viewers/HexViewer';
import type { DeletedFileRecovery, RecoveryProvenanceRange } from '@/types/models';
import type { DeletedRecoveryViewModel } from '../types';

const HASH_ALGORITHM_LABELS = {
  md5: 'MD5',
  sha1: 'SHA-1',
  sha256: 'SHA-256',
} as const;

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function rangeLabel(range: RecoveryProvenanceRange) {
  const end = range.logicalOffset + range.length;
  return `#${range.ordinal} 0x${range.logicalOffset.toString(16).toUpperCase()}-0x${Math.max(range.logicalOffset, end - 1).toString(16).toUpperCase()}`;
}

function CandidateDetail({ model }: { model: DeletedRecoveryViewModel }) {
  const { t } = useTranslation();
  const recovery = model.selectedRecovery;
  if (!recovery) {
    return <EmptyState className="m-3">{t('recovery.empty.selectCandidate')}</EmptyState>;
  }

  return (
    <ScrollArea className="min-h-0 flex-1" viewportClassName="p-3">
      <div className="flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-2 text-[11px]">
        <KeyValueField label="Inode / MFT" value={recovery.inode} mono />
        {recovery.mftSequence !== undefined ? (
          <KeyValueField label="MFT sequence" value={String(recovery.mftSequence)} mono />
        ) : null}
        <KeyValueField label={t('recovery.fields.completeness')} value={t(`recovery.completeness.${recovery.completeness}`)} />
        <KeyValueField label={t('recovery.fields.declaredSize')} value={formatBytes(recovery.declaredSize)} />
        <KeyValueField label={t('recovery.fields.recoverable')} value={formatBytes(recovery.recoverableBytes)} />
        <KeyValueField label={t('recovery.fields.method')} value={recovery.recoveryMethod} />
        <KeyValueField label={t('recovery.fields.confidence')} value={`${Math.round(recovery.confidence * 100)}%`} />
      </div>

      <KeyValueField
        label={t('recovery.fields.originalPath')}
        value={recovery.originalPath}
        mono
        valueClassName="break-all"
      />

      {recovery.contentMd5 || recovery.contentSha1 || recovery.contentSha256 ? (
        <div className="space-y-2 border-t border-forensics-border-light pt-3">
          {recovery.contentMd5 ? (
            <KeyValueField label="MD5" value={recovery.contentMd5} mono valueClassName="break-all" />
          ) : null}
          {recovery.contentSha1 ? (
            <KeyValueField label="SHA-1" value={recovery.contentSha1} mono valueClassName="break-all" />
          ) : null}
          {recovery.contentSha256 ? (
            <KeyValueField label="SHA-256" value={recovery.contentSha256} mono valueClassName="break-all" />
          ) : null}
        </div>
      ) : null}

      {recovery.warnings.length > 0 ? (
        <div className="border border-forensics-warning-border bg-forensics-warning-bg p-2 text-[11px] text-forensics-warning-text">
          {recovery.warnings.slice(0, 3).map((warning) => <div key={warning}>{warning}</div>)}
        </div>
      ) : null}

      {model.contentRanges.length > 0 ? (
        <div className="flex items-end gap-2">
          <div className="min-w-0 flex-1">
            <div className="mb-1 text-[10px] text-forensics-muted-light">{t('recovery.verifiedRanges')}</div>
            <Select
              value={model.selectedRangeOrdinal?.toString()}
              onValueChange={(value) => model.selectRange(Number(value))}
            >
              <SelectTrigger size="xs" variant="mono">
                <SelectValue placeholder={t('recovery.selectRange')} />
              </SelectTrigger>
              <SelectContent>
                {model.contentRanges.map((range) => (
                  <SelectItem key={range.ordinal} value={range.ordinal.toString()}>
                    {rangeLabel(range)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Button
            type="button"
            size="xs"
            variant="forensicsOutline"
            disabled={model.reading || model.selectedRangeOrdinal === undefined}
            onClick={model.readSelectedRange}
          >
            {model.reading ? <LoaderCircle className="animate-spin" /> : <Eye />}
            {t('recovery.read')}
          </Button>
        </div>
      ) : (
        <EmptyState className="p-3">{t('recovery.noVerifiedRange')}</EmptyState>
      )}

      {model.preview ? (
        <div className="h-64 min-h-64 overflow-hidden border border-forensics-border">
          <HexViewer
            lines={[]}
            rawBytes={model.preview.bytes}
            baseOffset={model.preview.offset}
            fileSize={model.preview.declaredSize}
            loadedRanges={[{
              start: model.preview.offset,
              end: model.preview.offset + model.preview.bytes.length,
            }]}
          />
        </div>
      ) : null}

      <Button
        type="button"
        size="sm"
        variant="forensicsPrimary"
        disabled={recovery.completeness !== 'complete' || model.exporting}
        onClick={model.exportSelected}
      >
        {model.exporting ? <LoaderCircle className="animate-spin" /> : <Download />}
        {t('recovery.exportFull')}
      </Button>

      {model.lastExport ? (
        <div className="space-y-1 border border-forensics-success-border bg-forensics-success-bg p-2 text-[10px] text-forensics-success-text">
          <div>{t('recovery.exported', { bytes: formatBytes(model.lastExport.bytesWritten) })}</div>
          <div className="break-all font-mono">SHA-256: {model.lastExport.sha256}</div>
        </div>
      ) : null}
      </div>
    </ScrollArea>
  );
}

// Module-level columns: stable reference keeps DenseDataTable row memoization
// intact across model state updates.
function recoveryColumns(t: ReturnType<typeof useTranslation>['t']): DenseColumn<DeletedFileRecovery>[] {
return [
  {
    key: 'partition',
    title: t('recovery.columns.partition'),
    className: 'w-[64px]',
    render: (row) => `P${row.partitionIndex}`,
  },
  {
    key: 'inode',
    title: t('recovery.columns.inode'),
    className: 'w-[110px]',
    render: (row) => row.inode,
  },
  {
    key: 'path',
    title: t('recovery.columns.originalPath'),
    className: 'min-w-[240px]',
    render: (row) => row.originalPath ?? '-',
  },
  {
    key: 'size',
    title: t('recovery.columns.size'),
    className: 'w-[90px]',
    render: (row) => formatBytes(row.declaredSize),
  },
  {
    key: 'completeness',
    title: t('recovery.columns.completeness'),
    className: 'w-[100px]',
    render: (row) => t(`recovery.completeness.${row.completeness}`),
  },
  {
    key: 'confidence',
    title: t('recovery.columns.confidence'),
    className: 'w-[75px]',
    render: (row) => `${Math.round(row.confidence * 100)}%`,
  },
];
}

export function DeletedRecoveryPanel({ model }: { model: DeletedRecoveryViewModel }) {
  const { t } = useTranslation();
  const columns = recoveryColumns(t);
  const handleRowClick = useCallback(
    (row: DeletedFileRecovery) => model.selectRecovery(row.id),
    [model],
  );
  return (
    <div className="flex h-full min-h-[36rem] flex-col gap-3">
      <SectionHeader
        icon={ScanSearch}
        title={t('recovery.title')}
        subtitle={t('recovery.subtitle')}
      />

      {model.partitions.length === 0 ? (
        <EmptyState>{t('recovery.noPartitions')}</EmptyState>
      ) : (
        <div className="flex items-end gap-3 border-b border-forensics-border-light pb-3">
          <div className="w-72">
            <div className="mb-1 text-[10px] text-forensics-muted-light">{t('recovery.targetPartition')}</div>
            <Select
              value={model.selectedPartitionIndex?.toString()}
              onValueChange={(value) => model.selectPartition(Number(value))}
            >
              <SelectTrigger size="sm" variant="forensics">
                <SelectValue placeholder={t('recovery.selectPartition')} />
              </SelectTrigger>
              <SelectContent>
                {model.partitions.map((partition) => (
                  <SelectItem key={partition.index} value={partition.index.toString()}>
                    P{partition.index} · {partition.name} · {partition.filesystem?.toUpperCase()}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Button
            type="button"
            size="sm"
            variant="forensicsPrimary"
            disabled={model.scanning || model.selectedPartitionIndex === undefined}
            onClick={model.runScan}
          >
            {model.scanning ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
            {model.page ? t('recovery.rescan') : t('recovery.startScan')}
          </Button>
          {model.page ? (
            <div className="ml-auto flex items-center gap-2 text-[11px] text-forensics-muted">
              <Badge variant="outline">{model.page.scan.filesystemType.toUpperCase()}</Badge>
                <span>{t('recovery.transactions', { count: model.page.scan.transactionCount })}</span>
                <span>{t('recovery.candidates', { count: model.total })}</span>
            </div>
          ) : null}
        </div>
      )}

      {model.partitions.length > 0 ? (
        <div className="flex flex-wrap items-start gap-2 border-b border-forensics-border-light pb-3">
          <div className="min-w-[20rem] max-w-2xl flex-1">
            <div className="relative">
              <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-forensics-muted-light" />
              <Input
                aria-label={t('recovery.hashSearch.label')}
                aria-invalid={Boolean(model.hashQuery) && !model.hashQueryValid}
                className="pl-7 pr-8"
                inputSize="compact"
                placeholder={t('recovery.hashSearch.placeholder')}
                spellCheck={false}
                value={model.hashQuery}
                variant="mono"
                onChange={(event) => model.setHashQuery(event.target.value.toLowerCase())}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && model.hashQueryValid && !model.hashSearching) {
                    model.runHashSearch();
                  }
                }}
              />
              {model.hashQuery ? (
                <Button
                  aria-label={t('recovery.hashSearch.clear')}
                  className="absolute right-0.5 top-0.5"
                  size="iconXs"
                  title={t('recovery.hashSearch.clear')}
                  type="button"
                  variant="forensicsGhost"
                  onClick={model.clearHashSearch}
                >
                  <X />
                </Button>
              ) : null}
            </div>
            {model.hashQuery && !model.hashQueryValid ? (
              <div className="mt-1 text-[10px] text-forensics-error-text">
                {t('recovery.hashSearch.invalid')}
              </div>
            ) : null}
            {model.hashSearchError ? (
              <div className="mt-1 text-[10px] text-forensics-error-text">{model.hashSearchError}</div>
            ) : null}
          </div>
          <Button
            disabled={!model.hashQueryValid || model.hashSearching}
            size="sm"
            type="button"
            variant="forensicsOutline"
            onClick={model.runHashSearch}
          >
            {model.hashSearching ? <LoaderCircle className="animate-spin" /> : <Search />}
            {t('recovery.hashSearch.submit')}
          </Button>
          {model.hashSearch ? (
            <Badge variant="outline" className="mt-1">
              {HASH_ALGORITHM_LABELS[model.hashSearch.algorithm]} · {t('recovery.hashSearch.matches', { count: model.hashSearch.matches.length })}
            </Badge>
          ) : null}
        </div>
      ) : null}

      {model.error ? (
        <div className="border border-forensics-error-border bg-forensics-error-bg p-3 text-[11px] text-forensics-error-text">
          {model.error}
        </div>
      ) : null}

      {model.failures.map((failure) => (
        <div key={`${failure.partitionIndex}:${failure.code}`} className="border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text">
          P{failure.partitionIndex} {failure.filesystemType}: {failure.message}
        </div>
      ))}

      {model.state === 'loading' || model.scanning ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-[12px] text-forensics-muted">
          <LoaderCircle className="animate-spin" />
          {t('recovery.loading')}
        </div>
      ) : model.state === 'unscanned' ? (
        <EmptyState className="flex-1">{t('recovery.unscanned')}</EmptyState>
      ) : model.state === 'ready' ? (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_22rem] overflow-hidden border border-forensics-border">
          <div className="flex min-h-0 min-w-0 flex-col border-r border-forensics-border">
            <DenseDataTableFrame layout="fill" variant="plain">
              <DenseDataTable
              columns={columns}
              rows={model.recoveries}
              getRowKey={(row) => row.id}
              selectedRowKey={model.selectedRecoveryId}
              onRowClick={handleRowClick}
              emptyTitle={t('recovery.empty.noCandidates')}
              emptyDescription={t('recovery.empty.noCandidatesDescription')}
              />
            </DenseDataTableFrame>
            <div className="flex shrink-0 items-center justify-between border-t border-forensics-border px-2 py-1 text-[10px] text-forensics-muted">
              <span>
                {model.hashSearch
                  ? t('recovery.pagination.hashMatches', { shown: model.recoveries.length, total: model.total })
                  : t('recovery.pagination.range', { start: model.page?.offset ?? 0, end: (model.page?.offset ?? 0) + model.recoveries.length, total: model.total })}
              </span>
              <div className="flex gap-1">
                <Button type="button" size="xs" variant="forensicsGhost" disabled={!model.hasPreviousPage} onClick={model.previousPage}>{t('recovery.pagination.previous')}</Button>
                <Button type="button" size="xs" variant="forensicsGhost" disabled={!model.hasNextPage} onClick={model.nextPage}>{t('recovery.pagination.next')}</Button>
              </div>
            </div>
          </div>
          <CandidateDetail model={model} />
        </div>
      ) : null}
    </div>
  );
}
