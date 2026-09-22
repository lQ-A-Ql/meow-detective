import type {
  BrowserCookie,
  BrowserDownload,
  BrowserHistorySummary,
  BrowserPassword,
  BrowserSessionTab,
  BrowserVisit,
} from '@/types/models';
import { useTranslation } from 'react-i18next';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import {
  ExtractionTableSection,
  formatSize,
  TableBlock,
} from './helpers';

function groupByBrowser<T extends { browser: string }>(rows: T[]): Record<string, T[]> {
  const groups: Record<string, T[]> = {};
  for (const row of rows) {
    (groups[row.browser] ??= []).push(row);
  }
  return groups;
}

function browserOrder(a: string, b: string): number {
  const order = ['Chrome', 'Edge', 'Firefox'];
  const ai = order.indexOf(a);
  const bi = order.indexOf(b);
  if (ai !== -1 && bi !== -1) return ai - bi;
  if (ai !== -1) return -1;
  if (bi !== -1) return 1;
  return a.localeCompare(b);
}

// Module-level columns: stable references keep DenseDataTable's memoized rows
// from re-rendering whenever panel state changes above the tables.
function browserColumns(t: ReturnType<typeof useTranslation>['t']) {
const visitColumns: DenseColumn<BrowserVisit>[] = [
  { key: 'visitTime', title: t('browser.columns.time'), className: 'w-[170px]', render: (row) => row.visitTime ?? '-' },
  { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
  { key: 'title', title: t('browser.columns.title'), className: 'min-w-[220px]', render: (row) => row.title || '-' },
  { key: 'url', title: 'URL', className: 'min-w-[300px]', render: (row) => row.url },
  { key: 'visitCount', title: t('browser.columns.count'), className: 'w-[70px]', render: (row) => row.visitCount.toString() },
];
const downloadColumns: DenseColumn<BrowserDownload>[] = [
  { key: 'startTime', title: t('browser.columns.time'), className: 'w-[170px]', render: (row) => row.startTime ?? '-' },
  { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
  { key: 'targetPath', title: t('browser.columns.targetPath'), className: 'min-w-[260px]', render: (row) => row.targetPath || '-' },
  { key: 'url', title: 'URL', className: 'min-w-[260px]', render: (row) => row.url || '-' },
  { key: 'totalBytes', title: t('browser.columns.size'), className: 'w-[110px]', render: (row) => formatSize(row.totalBytes) },
];
const cookieColumns: DenseColumn<BrowserCookie>[] = [
  { key: 'profile', title: 'Profile', className: 'w-[120px]', render: (row) => row.profile || '-' },
  { key: 'domain', title: t('browser.columns.domain'), className: 'min-w-[220px]', render: (row) => row.domain },
  { key: 'name', title: t('browser.columns.name'), className: 'min-w-[180px]', render: (row) => row.name },
  { key: 'valuePreview', title: t('browser.columns.valuePreview'), className: 'min-w-[220px]', render: (row) => row.valuePreview ?? '-' },
  { key: 'expiry', title: t('browser.columns.expiry'), className: 'w-[170px]', render: (row) => row.expiry ?? '-' },
  { key: 'secure', title: t('browser.columns.secure'), className: 'w-[80px]', render: (row) => (row.secure ? t('common.yes') : t('common.no')) },
  { key: 'httpOnly', title: t('browser.columns.httpOnly'), className: 'w-[80px]', render: (row) => (row.httpOnly ? t('common.yes') : t('common.no')) },
  { key: 'decryptionStatus', title: t('browser.columns.decryptionStatus'), className: 'w-[110px]', render: (row) => row.decryptionStatus ?? t('common.unknown') },
];
const sessionColumns: DenseColumn<BrowserSessionTab>[] = [
  { key: 'title', title: t('browser.columns.title'), className: 'min-w-[220px]', render: (row) => row.title ?? '-' },
  { key: 'url', title: 'URL', className: 'min-w-[300px]', render: (row) => row.url },
  { key: 'windowIndex', title: t('browser.columns.window'), className: 'w-[70px]', render: (row) => row.windowIndex.toString() },
  { key: 'tabIndex', title: t('browser.columns.tab'), className: 'w-[70px]', render: (row) => row.tabIndex.toString() },
  { key: 'lastActive', title: t('browser.columns.lastActive'), className: 'w-[170px]', render: (row) => row.lastActive ?? '-' },
];
const passwordColumns: DenseColumn<BrowserPassword>[] = [
  { key: 'url', title: t('browser.columns.website'), className: 'min-w-[260px]', render: (row) => row.url },
  { key: 'username', title: t('browser.columns.username'), className: 'min-w-[180px]', render: (row) => row.username },
  { key: 'passwordPreview', title: t('browser.columns.passwordPreview'), className: 'min-w-[160px]', render: (row) => row.passwordPreview ?? '-' },
  { key: 'createdAt', title: t('browser.columns.createdAt'), className: 'w-[170px]', render: (row) => row.createdAt ?? '-' },
  { key: 'timesUsed', title: t('browser.columns.timesUsed'), className: 'w-[90px]', render: (row) => row.timesUsed.toString() },
  { key: 'decryptionStatus', title: t('browser.columns.decryptionStatus'), className: 'w-[110px]', render: (row) => row.decryptionStatus ?? t('common.unknown') },
];
return { visitColumns, downloadColumns, cookieColumns, sessionColumns, passwordColumns };
}

export function BrowserHistoryPanel({
  summary,
}: {
  summary?: BrowserHistorySummary;
}) {
  const { t } = useTranslation();
  const { visitColumns, downloadColumns, cookieColumns, sessionColumns, passwordColumns } = browserColumns(t);
  const info = summary ?? {
    status: 'unavailable' as const,
    visitTotal: 0,
    downloadTotal: 0,
    cookieTotal: 0,
    sessionTotal: 0,
    passwordTotal: 0,
    visits: [],
    downloads: [],
    cookies: [],
    sessions: [],
    passwords: [],
    generatedAt: '',
    warnings: [t('browser.unavailable')],
  };

  const visitGroups = groupByBrowser(info.visits);
  const downloadGroups = groupByBrowser(info.downloads);
  const cookieGroups = groupByBrowser(info.cookies);
  const sessionGroups = groupByBrowser(info.sessions);
  const passwordGroups = groupByBrowser(info.passwords);

  return (
    <ExtractionTableSection
      title={t('browser.title')}
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        [t('browser.stats.visits'), info.visitTotal.toString()],
        [t('browser.stats.downloads'), info.downloadTotal.toString()],
        [t('browser.stats.cookies'), info.cookieTotal.toString()],
        [t('browser.stats.sessions'), info.sessionTotal.toString()],
        [t('browser.stats.passwords'), info.passwordTotal.toString()],
        [t('browser.stats.browsers'), Object.keys(visitGroups).sort(browserOrder).join(' / ') || '-'],
      ]}
    >
      <div className="space-y-4">
        <TableBlock title={t('browser.sections.visits')}>
          {info.visits.length === 0 ? (
            <DenseDataTableFrame rowCount={0}>
              <DenseDataTable
                rows={[]}
                columns={visitColumns}
                getRowKey={() => 'empty'}
                emptyTitle={t('browser.empty.visits')}
                emptyDescription={t('browser.empty.visitsDescription')}
              />
            </DenseDataTableFrame>
          ) : (
            Object.entries(visitGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseDataTableFrame
                  key={`visits-${browser}`}
                  rowCount={rows.length}
                  header={<div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>}
                >
                  <DenseDataTable
                    rows={rows}
                    columns={visitColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle={t('browser.empty.visits')}
                    emptyDescription=""
                  />
                </DenseDataTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title={t('browser.sections.downloads')}>
          {info.downloads.length === 0 ? (
            <DenseDataTableFrame rowCount={0}>
              <DenseDataTable
                rows={[]}
                columns={downloadColumns}
                getRowKey={() => 'empty'}
                emptyTitle={t('browser.empty.downloads')}
                emptyDescription={t('browser.empty.downloadsDescription')}
              />
            </DenseDataTableFrame>
          ) : (
            Object.entries(downloadGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseDataTableFrame
                  key={`downloads-${browser}`}
                  rowCount={rows.length}
                  header={<div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>}
                >
                  <DenseDataTable
                    rows={rows}
                    columns={downloadColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle={t('browser.empty.downloads')}
                    emptyDescription=""
                  />
                </DenseDataTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title={t('browser.sections.cookies')}>
          {info.cookies.length === 0 ? (
            <DenseDataTableFrame rowCount={0}>
              <DenseDataTable
                rows={[]}
                columns={cookieColumns}
                getRowKey={() => 'empty'}
                emptyTitle={t('browser.empty.cookies')}
                emptyDescription={t('browser.empty.cookiesDescription')}
              />
            </DenseDataTableFrame>
          ) : (
            Object.entries(cookieGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseDataTableFrame
                  key={`cookies-${browser}`}
                  rowCount={rows.length}
                  header={<div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>}
                >
                  <DenseDataTable
                    rows={rows}
                    columns={cookieColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle={t('browser.empty.cookies')}
                    emptyDescription=""
                  />
                </DenseDataTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title={t('browser.sections.sessions')}>
          {info.sessions.length === 0 ? (
            <DenseDataTableFrame rowCount={0}>
              <DenseDataTable
                rows={[]}
                columns={sessionColumns}
                getRowKey={() => 'empty'}
                emptyTitle={t('browser.empty.sessions')}
                emptyDescription={t('browser.empty.sessionsDescription')}
              />
            </DenseDataTableFrame>
          ) : (
            Object.entries(sessionGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseDataTableFrame
                  key={`sessions-${browser}`}
                  rowCount={rows.length}
                  header={<div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>}
                >
                  <DenseDataTable
                    rows={rows}
                    columns={sessionColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle={t('browser.empty.sessions')}
                    emptyDescription=""
                  />
                </DenseDataTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title={t('browser.sections.passwords')}>
          {info.passwords.length === 0 ? (
            <DenseDataTableFrame rowCount={0}>
              <DenseDataTable
                rows={[]}
                columns={passwordColumns}
                getRowKey={() => 'empty'}
                emptyTitle={t('browser.empty.passwords')}
                emptyDescription={t('browser.empty.passwordsDescription')}
              />
            </DenseDataTableFrame>
          ) : (
            Object.entries(passwordGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseDataTableFrame
                  key={`passwords-${browser}`}
                  rowCount={rows.length}
                  header={<div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>}
                >
                  <DenseDataTable
                    rows={rows}
                    columns={passwordColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle={t('browser.empty.passwords')}
                    emptyDescription=""
                  />
                </DenseDataTableFrame>
              ))
          )}
        </TableBlock>
      </div>
    </ExtractionTableSection>
  );
}
