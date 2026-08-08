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

const COMPLETENESS_LABELS: Record<DeletedFileRecovery['completeness'], string> = {
  metadata_only: '仅元数据',
  partial: '部分内容',
  complete: '完整内容',
};

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
  const recovery = model.selectedRecovery;
  if (!recovery) {
    return <EmptyState className="m-3">选择候选记录查看取证元数据</EmptyState>;
  }

  return (
    <ScrollArea className="min-h-0 flex-1" viewportClassName="p-3">
      <div className="flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-2 text-[11px]">
        <KeyValueField label="Inode / MFT" value={recovery.inode} mono />
        {recovery.mftSequence !== undefined ? (
          <KeyValueField label="MFT sequence" value={String(recovery.mftSequence)} mono />
        ) : null}
        <KeyValueField label="完整度" value={COMPLETENESS_LABELS[recovery.completeness]} />
        <KeyValueField label="声明大小" value={formatBytes(recovery.declaredSize)} />
        <KeyValueField label="可恢复" value={formatBytes(recovery.recoverableBytes)} />
        <KeyValueField label="恢复方法" value={recovery.recoveryMethod} />
        <KeyValueField label="置信度" value={`${Math.round(recovery.confidence * 100)}%`} />
      </div>

      <KeyValueField
        label="原始路径"
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
            <div className="mb-1 text-[10px] text-forensics-muted-light">已验证内容区间</div>
            <Select
              value={model.selectedRangeOrdinal?.toString()}
              onValueChange={(value) => model.selectRange(Number(value))}
            >
              <SelectTrigger size="xs" variant="mono">
                <SelectValue placeholder="选择区间" />
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
            读取
          </Button>
        </div>
      ) : (
        <EmptyState className="p-3">该候选没有可读取的已验证内容区间</EmptyState>
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
        导出完整恢复文件
      </Button>

      {model.lastExport ? (
        <div className="space-y-1 border border-forensics-success-border bg-forensics-success-bg p-2 text-[10px] text-forensics-success-text">
          <div>已导出 {formatBytes(model.lastExport.bytesWritten)}</div>
          <div className="break-all font-mono">SHA-256: {model.lastExport.sha256}</div>
        </div>
      ) : null}
      </div>
    </ScrollArea>
  );
}

// Module-level columns: stable reference keeps DenseDataTable row memoization
// intact across model state updates.
const columns: DenseColumn<DeletedFileRecovery>[] = [
  {
    key: 'partition',
    title: '分区',
    className: 'w-[64px]',
    render: (row) => `P${row.partitionIndex}`,
  },
  {
    key: 'inode',
    title: 'Inode',
    className: 'w-[110px]',
    render: (row) => row.inode,
  },
  {
    key: 'path',
    title: '原始路径',
    className: 'min-w-[240px]',
    render: (row) => row.originalPath ?? '-',
  },
  {
    key: 'size',
    title: '大小',
    className: 'w-[90px]',
    render: (row) => formatBytes(row.declaredSize),
  },
  {
    key: 'completeness',
    title: '恢复状态',
    className: 'w-[100px]',
    render: (row) => COMPLETENESS_LABELS[row.completeness],
  },
  {
    key: 'confidence',
    title: '置信度',
    className: 'w-[75px]',
    render: (row) => `${Math.round(row.confidence * 100)}%`,
  },
];

export function DeletedRecoveryPanel({ model }: { model: DeletedRecoveryViewModel }) {
  const handleRowClick = useCallback(
    (row: DeletedFileRecovery) => model.selectRecovery(row.id),
    [model],
  );
  return (
    <div className="flex h-full min-h-[36rem] flex-col gap-3">
      <SectionHeader
        icon={ScanSearch}
        title="删除文件恢复"
        subtitle="NTFS MFT / EXT4 journal / XFS log"
      />

      {model.partitions.length === 0 ? (
        <EmptyState>当前数据源没有可执行删除恢复的 NTFS/EXT4/XFS 分区</EmptyState>
      ) : (
        <div className="flex items-end gap-3 border-b border-forensics-border-light pb-3">
          <div className="w-72">
            <div className="mb-1 text-[10px] text-forensics-muted-light">目标分区</div>
            <Select
              value={model.selectedPartitionIndex?.toString()}
              onValueChange={(value) => model.selectPartition(Number(value))}
            >
              <SelectTrigger size="sm" variant="forensics">
                <SelectValue placeholder="选择 NTFS/EXT4/XFS 分区" />
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
            {model.page ? '重新扫描' : '开始扫描'}
          </Button>
          {model.page ? (
            <div className="ml-auto flex items-center gap-2 text-[11px] text-forensics-muted">
              <Badge variant="outline">{model.page.scan.filesystemType.toUpperCase()}</Badge>
              <span>{model.page.scan.transactionCount} transactions</span>
              <span>{model.total} candidates</span>
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
                aria-label="恢复文件哈希"
                aria-invalid={Boolean(model.hashQuery) && !model.hashQueryValid}
                className="pl-7 pr-8"
                inputSize="compact"
                placeholder="MD5 / SHA-1 / SHA-256"
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
                  aria-label="清除哈希搜索"
                  className="absolute right-0.5 top-0.5"
                  size="iconXs"
                  title="清除哈希搜索"
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
                哈希必须是 32、40 或 64 位十六进制值
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
            按哈希查找
          </Button>
          {model.hashSearch ? (
            <Badge variant="outline" className="mt-1">
              {HASH_ALGORITHM_LABELS[model.hashSearch.algorithm]} · {model.hashSearch.matches.length} 项匹配
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
          正在读取恢复扫描结果
        </div>
      ) : model.state === 'unscanned' ? (
        <EmptyState className="flex-1">当前分区尚未执行删除恢复扫描</EmptyState>
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
              emptyTitle="没有删除恢复候选"
              emptyDescription="当前扫描未重建出可报告的删除记录。"
              />
            </DenseDataTableFrame>
            <div className="flex shrink-0 items-center justify-between border-t border-forensics-border px-2 py-1 text-[10px] text-forensics-muted">
              <span>
                {model.hashSearch
                  ? `哈希匹配 ${model.recoveries.length} / ${model.total}`
                  : `显示 ${model.page?.offset ?? 0}-${(model.page?.offset ?? 0) + model.recoveries.length} / ${model.total}`}
              </span>
              <div className="flex gap-1">
                <Button type="button" size="xs" variant="forensicsGhost" disabled={!model.hasPreviousPage} onClick={model.previousPage}>上一组</Button>
                <Button type="button" size="xs" variant="forensicsGhost" disabled={!model.hasNextPage} onClick={model.nextPage}>下一组</Button>
              </div>
            </div>
          </div>
          <CandidateDetail model={model} />
        </div>
      ) : null}
    </div>
  );
}
