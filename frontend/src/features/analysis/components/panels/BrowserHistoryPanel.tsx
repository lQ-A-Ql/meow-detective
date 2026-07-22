import type {
  BrowserCookie,
  BrowserDownload,
  BrowserHistorySummary,
  BrowserPassword,
  BrowserSessionTab,
  BrowserVisit,
} from '@/types/models';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  DenseTableFrame,
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

export function BrowserHistoryPanel({
  summary,
}: {
  summary?: BrowserHistorySummary;
}) {
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
    warnings: ['浏览器记录暂不可用。'],
  };

  const visitColumns: DenseColumn<BrowserVisit>[] = [
    { key: 'visitTime', title: '时间', className: 'w-[170px]', render: (row) => row.visitTime ?? '-' },
    { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
    { key: 'title', title: '标题', className: 'min-w-[220px]', render: (row) => row.title || '-' },
    { key: 'url', title: 'URL', className: 'min-w-[300px]', render: (row) => row.url },
    { key: 'visitCount', title: '次数', className: 'w-[70px]', render: (row) => row.visitCount.toString() },
  ];
  const downloadColumns: DenseColumn<BrowserDownload>[] = [
    { key: 'startTime', title: '时间', className: 'w-[170px]', render: (row) => row.startTime ?? '-' },
    { key: 'profile', title: 'Profile', className: 'w-[130px]', render: (row) => row.profile || '-' },
    { key: 'targetPath', title: '目标路径', className: 'min-w-[260px]', render: (row) => row.targetPath || '-' },
    { key: 'url', title: 'URL', className: 'min-w-[260px]', render: (row) => row.url || '-' },
    { key: 'totalBytes', title: '大小', className: 'w-[110px]', render: (row) => formatSize(row.totalBytes) },
  ];
  const cookieColumns: DenseColumn<BrowserCookie>[] = [
    { key: 'profile', title: 'Profile', className: 'w-[120px]', render: (row) => row.profile || '-' },
    { key: 'domain', title: '域名', className: 'min-w-[220px]', render: (row) => row.domain },
    { key: 'name', title: '名称', className: 'min-w-[180px]', render: (row) => row.name },
    { key: 'valuePreview', title: '值预览', className: 'min-w-[220px]', render: (row) => row.valuePreview ?? '-' },
    { key: 'expiry', title: '过期时间', className: 'w-[170px]', render: (row) => row.expiry ?? '-' },
    { key: 'secure', title: '安全标记', className: 'w-[80px]', render: (row) => (row.secure ? '是' : '否') },
    { key: 'httpOnly', title: '仅 HTTP', className: 'w-[80px]', render: (row) => (row.httpOnly ? '是' : '否') },
    { key: 'decryptionStatus', title: '解密状态', className: 'w-[110px]', render: (row) => row.decryptionStatus ?? '未知' },
  ];
  const sessionColumns: DenseColumn<BrowserSessionTab>[] = [
    { key: 'title', title: '标题', className: 'min-w-[220px]', render: (row) => row.title ?? '-' },
    { key: 'url', title: 'URL', className: 'min-w-[300px]', render: (row) => row.url },
    { key: 'windowIndex', title: '窗口', className: 'w-[70px]', render: (row) => row.windowIndex.toString() },
    { key: 'tabIndex', title: '标签', className: 'w-[70px]', render: (row) => row.tabIndex.toString() },
    { key: 'lastActive', title: '最后活跃', className: 'w-[170px]', render: (row) => row.lastActive ?? '-' },
  ];
  const passwordColumns: DenseColumn<BrowserPassword>[] = [
    { key: 'url', title: '网站', className: 'min-w-[260px]', render: (row) => row.url },
    { key: 'username', title: '用户名', className: 'min-w-[180px]', render: (row) => row.username },
    { key: 'passwordPreview', title: '密码预览', className: 'min-w-[160px]', render: (row) => row.passwordPreview ?? '-' },
    { key: 'createdAt', title: '创建时间', className: 'w-[170px]', render: (row) => row.createdAt ?? '-' },
    { key: 'timesUsed', title: '使用次数', className: 'w-[90px]', render: (row) => row.timesUsed.toString() },
    { key: 'decryptionStatus', title: '解密状态', className: 'w-[110px]', render: (row) => row.decryptionStatus ?? '未知' },
  ];

  const visitGroups = groupByBrowser(info.visits);
  const downloadGroups = groupByBrowser(info.downloads);
  const cookieGroups = groupByBrowser(info.cookies);
  const sessionGroups = groupByBrowser(info.sessions);
  const passwordGroups = groupByBrowser(info.passwords);

  return (
    <ExtractionTableSection
      title="浏览器记录"
      status={info.status}
      generatedAt={info.generatedAt}
      warnings={info.warnings}
      stats={[
        ['访问记录', info.visitTotal.toString()],
        ['下载记录', info.downloadTotal.toString()],
        ['Cookie', info.cookieTotal.toString()],
        ['会话标签', info.sessionTotal.toString()],
        ['密码项', info.passwordTotal.toString()],
        ['浏览器', Object.keys(visitGroups).sort(browserOrder).join(' / ') || '-'],
      ]}
    >
      <div className="space-y-4">
        <TableBlock title="访问历史">
          {info.visits.length === 0 ? (
            <DenseTableFrame>
              <DenseDataTable
                rows={[]}
                columns={visitColumns}
                getRowKey={() => 'empty'}
                emptyTitle="暂无浏览历史"
                emptyDescription="支持 Chrome、Edge History 与 Firefox places.sqlite。"
              />
            </DenseTableFrame>
          ) : (
            Object.entries(visitGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseTableFrame key={`visits-${browser}`}>
                  <div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>
                  <DenseDataTable
                    rows={rows}
                    columns={visitColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle="暂无浏览历史"
                    emptyDescription=""
                  />
                </DenseTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title="下载记录">
          {info.downloads.length === 0 ? (
            <DenseTableFrame>
              <DenseDataTable
                rows={[]}
                columns={downloadColumns}
                getRowKey={() => 'empty'}
                emptyTitle="暂无下载记录"
                emptyDescription="发现下载记录后会显示 URL、目标路径与大小。"
              />
            </DenseTableFrame>
          ) : (
            Object.entries(downloadGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseTableFrame key={`downloads-${browser}`}>
                  <div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>
                  <DenseDataTable
                    rows={rows}
                    columns={downloadColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle="暂无下载记录"
                    emptyDescription=""
                  />
                </DenseTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title="Cookies">
          {info.cookies.length === 0 ? (
            <DenseTableFrame>
              <DenseDataTable
                rows={[]}
                columns={cookieColumns}
                getRowKey={() => 'empty'}
                emptyTitle="暂无 Cookie 记录"
                emptyDescription="发现 cookies 数据库后会显示域名、名称与过期时间。"
              />
            </DenseTableFrame>
          ) : (
            Object.entries(cookieGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseTableFrame key={`cookies-${browser}`}>
                  <div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>
                  <DenseDataTable
                    rows={rows}
                    columns={cookieColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle="暂无 Cookie 记录"
                    emptyDescription=""
                  />
                </DenseTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title="会话 / 标签页">
          {info.sessions.length === 0 ? (
            <DenseTableFrame>
              <DenseDataTable
                rows={[]}
                columns={sessionColumns}
                getRowKey={() => 'empty'}
                emptyTitle="暂无会话记录"
                emptyDescription="发现 Session/Session Restore 文件后会显示标签页 URL。"
              />
            </DenseTableFrame>
          ) : (
            Object.entries(sessionGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseTableFrame key={`sessions-${browser}`}>
                  <div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>
                  <DenseDataTable
                    rows={rows}
                    columns={sessionColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle="暂无会话记录"
                    emptyDescription=""
                  />
                </DenseTableFrame>
              ))
          )}
        </TableBlock>

        <TableBlock title="保存的密码">
          {info.passwords.length === 0 ? (
            <DenseTableFrame>
              <DenseDataTable
                rows={[]}
                columns={passwordColumns}
                getRowKey={() => 'empty'}
                emptyTitle="暂无密码记录"
                emptyDescription="发现 login data / logins.json 后仅展示 URL、用户名等元数据。"
              />
            </DenseTableFrame>
          ) : (
            Object.entries(passwordGroups)
              .sort(([a], [b]) => browserOrder(a, b))
              .map(([browser, rows]) => (
                <DenseTableFrame key={`passwords-${browser}`}>
                  <div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>
                  <DenseDataTable
                    rows={rows}
                    columns={passwordColumns}
                    getRowKey={(row) => row.artifactId}
                    emptyTitle="暂无密码记录"
                    emptyDescription=""
                  />
                </DenseTableFrame>
              ))
          )}
        </TableBlock>
      </div>
    </ExtractionTableSection>
  );
}
