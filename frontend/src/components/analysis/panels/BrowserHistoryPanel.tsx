import type {
  BrowserDownload,
  BrowserHistorySummary,
  BrowserVisit,
} from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  AnalysisExtractionProgress,
  type AnalysisExtractionProgressInfo,
  DenseTableFrame,
  ExtractionTableSection,
  formatSize,
  TableBlock,
} from './helpers';

export function BrowserHistoryPanel({
  summary,
  progress,
}: {
  summary?: BrowserHistorySummary;
  progress?: AnalysisExtractionProgressInfo;
}) {
  const info = summary ?? {
    status: 'unavailable' as const,
    visitTotal: 0,
    downloadTotal: 0,
    visits: [],
    downloads: [],
    generatedAt: '',
    warnings: ['浏览器记录暂不可用。'],
  };
  const visitColumns: DenseColumn<BrowserVisit>[] = [
    { key: 'visitTime', title: '时间', className: 'w-[170px]', render: (row) => row.visitTime ?? '-' },
    { key: 'browser', title: '浏览器', className: 'w-[90px]', render: (row) => row.browser },
    { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
    { key: 'title', title: '标题', className: 'min-w-[220px]', render: (row) => row.title || '-' },
    { key: 'url', title: 'URL', className: 'min-w-[300px]', render: (row) => row.url },
    { key: 'visitCount', title: '次数', className: 'w-[70px]', render: (row) => row.visitCount.toString() },
  ];
  const downloadColumns: DenseColumn<BrowserDownload>[] = [
    { key: 'startTime', title: '时间', className: 'w-[170px]', render: (row) => row.startTime ?? '-' },
    { key: 'browser', title: '浏览器', className: 'w-[90px]', render: (row) => row.browser },
    { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
    { key: 'targetPath', title: '目标路径', className: 'min-w-[260px]', render: (row) => row.targetPath || '-' },
    { key: 'url', title: 'URL', className: 'min-w-[260px]', render: (row) => row.url || '-' },
    { key: 'totalBytes', title: '大小', className: 'w-[110px]', render: (row) => formatSize(row.totalBytes) },
  ];

  return (
    <ExtractionTableSection
      title="浏览器记录"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['访问记录', info.visitTotal.toString()],
        ['下载记录', info.downloadTotal.toString()],
        ['浏览器', Array.from(new Set(info.visits.map((item) => item.browser))).join(' / ') || '-'],
      ]}
    >
      <AnalysisExtractionProgress progress={progress} />
      <div className="space-y-4">
        <TableBlock title="访问历史">
          <DenseTableFrame>
            <DenseDataTable
              rows={info.visits}
              columns={visitColumns}
              getRowKey={(row) => row.artifactId}
              emptyTitle="暂无浏览历史"
              emptyDescription="支持 Chrome、Edge History 与 Firefox places.sqlite。"
            />
          </DenseTableFrame>
        </TableBlock>
        <TableBlock title="下载记录">
          <DenseTableFrame>
            <DenseDataTable
              rows={info.downloads}
              columns={downloadColumns}
              getRowKey={(row) => row.artifactId}
              emptyTitle="暂无下载记录"
              emptyDescription="发现下载记录后会显示 URL、目标路径与大小。"
            />
          </DenseTableFrame>
        </TableBlock>
      </div>
    </ExtractionTableSection>
  );
}
