import type {
  BrowserCookie,
  BrowserDownload,
  BrowserExtension,
  BrowserHistorySummary,
  BrowserPassword,
  BrowserSessionTab,
  BrowserVisit,
} from '@/types/models';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/app/components/ui/dialog';
import { Input } from '@/app/components/ui/input';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { ExtractionTableSection, formatSize, TableBlock } from './helpers';

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
    {
      key: 'recordKind',
      title: t('browser.recordKind'),
      className: 'w-[110px]',
      render: (row) => (row.recordKind ? t('browser.recordKinds.' + row.recordKind) : t('common.unknown')),
    },
    {
      key: 'visitTime',
      title: t('browser.columns.time'),
      className: 'w-[170px]',
      render: (row) => row.visitTime ?? '-',
    },
    {
      key: 'profile',
      title: t('browser.columns.profile'),
      className: 'w-[130px]',
      render: (row) => row.profile || '-',
    },
    { key: 'title', title: t('browser.columns.title'), className: 'min-w-[220px]', render: (row) => row.title || '-' },
    { key: 'url', title: t('browser.columns.url'), className: 'min-w-[300px]', render: (row) => row.url },
    {
      key: 'visitCount',
      title: t('browser.columns.count'),
      className: 'w-[70px]',
      render: (row) => row.visitCount.toString(),
    },
  ];
  const downloadColumns: DenseColumn<BrowserDownload>[] = [
    {
      key: 'startTime',
      title: t('browser.columns.time'),
      className: 'w-[170px]',
      render: (row) => row.startTime ?? '-',
    },
    {
      key: 'profile',
      title: t('browser.columns.profile'),
      className: 'w-[130px]',
      render: (row) => row.profile || '-',
    },
    {
      key: 'targetPath',
      title: t('browser.columns.targetPath'),
      className: 'min-w-[260px]',
      render: (row) => row.targetPath || '-',
    },
    { key: 'url', title: t('browser.columns.url'), className: 'min-w-[260px]', render: (row) => row.url || '-' },
    {
      key: 'totalBytes',
      title: t('browser.columns.size'),
      className: 'w-[110px]',
      render: (row) => formatSize(row.totalBytes),
    },
  ];
  const cookieColumns: DenseColumn<BrowserCookie>[] = [
    {
      key: 'profile',
      title: t('browser.columns.profile'),
      className: 'w-[120px]',
      render: (row) => row.profile || '-',
    },
    { key: 'domain', title: t('browser.columns.domain'), className: 'min-w-[220px]', render: (row) => row.domain },
    { key: 'name', title: t('browser.columns.name'), className: 'min-w-[180px]', render: (row) => row.name },
    {
      key: 'valuePreview',
      title: t('browser.columns.valuePreview'),
      className: 'min-w-[220px]',
      render: (row) => row.valuePreview ?? '-',
    },
    { key: 'expiry', title: t('browser.columns.expiry'), className: 'w-[170px]', render: (row) => row.expiry ?? '-' },
    {
      key: 'secure',
      title: t('browser.columns.secure'),
      className: 'w-[80px]',
      render: (row) => (row.secure ? t('common.yes') : t('common.no')),
    },
    {
      key: 'httpOnly',
      title: t('browser.columns.httpOnly'),
      className: 'w-[80px]',
      render: (row) => (row.httpOnly ? t('common.yes') : t('common.no')),
    },
    {
      key: 'decryptionStatus',
      title: t('browser.columns.decryptionStatus'),
      className: 'w-[110px]',
      render: (row) => row.decryptionStatus ?? t('common.unknown'),
    },
  ];
  const sessionColumns: DenseColumn<BrowserSessionTab>[] = [
    { key: 'title', title: t('browser.columns.title'), className: 'min-w-[220px]', render: (row) => row.title ?? '-' },
    { key: 'url', title: t('browser.columns.url'), className: 'min-w-[300px]', render: (row) => row.url },
    {
      key: 'windowIndex',
      title: t('browser.columns.window'),
      className: 'w-[70px]',
      render: (row) => row.windowIndex.toString(),
    },
    {
      key: 'tabIndex',
      title: t('browser.columns.tab'),
      className: 'w-[70px]',
      render: (row) => row.tabIndex.toString(),
    },
    {
      key: 'lastActive',
      title: t('browser.columns.lastActive'),
      className: 'w-[170px]',
      render: (row) => row.lastActive ?? '-',
    },
  ];
  const passwordColumns: DenseColumn<BrowserPassword>[] = [
    { key: 'url', title: t('browser.columns.website'), className: 'min-w-[260px]', render: (row) => row.url },
    {
      key: 'username',
      title: t('browser.columns.username'),
      className: 'min-w-[180px]',
      render: (row) => row.username,
    },
    {
      key: 'passwordPreview',
      title: t('browser.columns.passwordPreview'),
      className: 'min-w-[160px]',
      render: (row) => row.passwordPreview ?? '-',
    },
    {
      key: 'createdAt',
      title: t('browser.columns.createdAt'),
      className: 'w-[170px]',
      render: (row) => row.createdAt ?? '-',
    },
    {
      key: 'timesUsed',
      title: t('browser.columns.timesUsed'),
      className: 'w-[90px]',
      render: (row) => row.timesUsed.toString(),
    },
    {
      key: 'decryptionStatus',
      title: t('browser.columns.decryptionStatus'),
      className: 'w-[110px]',
      render: (row) => row.decryptionStatus ?? t('common.unknown'),
    },
  ];
  const extensionColumns: DenseColumn<BrowserExtension>[] = [
    {
      key: 'name',
      title: t('browser.columns.extensionName'),
      className: 'min-w-[200px]',
      render: (row) => row.name || row.id,
    },
    {
      key: 'version',
      title: t('browser.columns.version'),
      className: 'w-[110px]',
      render: (row) => row.version || '-',
    },
    {
      key: 'profile',
      title: t('browser.columns.profile'),
      className: 'w-[150px]',
      render: (row) => row.profile || '-',
    },
    {
      key: 'active',
      title: t('browser.columns.active'),
      className: 'w-[90px]',
      render: (row) => (row.active ? t('common.yes') : t('common.no')),
    },
    {
      key: 'userDisabled',
      title: t('browser.columns.userDisabled'),
      className: 'w-[110px]',
      render: (row) => (row.userDisabled ? t('common.yes') : t('common.no')),
    },
    {
      key: 'signedState',
      title: t('browser.columns.signedState'),
      className: 'w-[110px]',
      render: (row) => row.signedState ?? '-',
    },
    {
      key: 'permissions',
      title: t('browser.columns.permissions'),
      className: 'min-w-[240px]',
      render: (row) => row.permissions.join(', ') || '-',
    },
  ];
  return { visitColumns, downloadColumns, cookieColumns, sessionColumns, passwordColumns, extensionColumns };
}

function BrowserTableSection<T extends { artifactId: string; browser: string }>({
  title,
  rows,
  columns,
  emptyTitle,
  emptyDescription,
}: {
  title: string;
  rows: T[];
  columns: DenseColumn<T>[];
  emptyTitle: string;
  emptyDescription: string;
}) {
  return (
    <TableBlock title={title}>
      {rows.length === 0 ? (
        <DenseDataTableFrame rowCount={0}>
          <DenseDataTable
            rows={[]}
            columns={columns}
            getRowKey={() => 'empty'}
            emptyTitle={emptyTitle}
            emptyDescription={emptyDescription}
          />
        </DenseDataTableFrame>
      ) : (
        Object.entries(groupByBrowser(rows))
          .sort(([a], [b]) => browserOrder(a, b))
          .map(([browser, groupedRows]) => (
            <DenseDataTableFrame
              key={browser}
              rowCount={groupedRows.length}
              header={<div className="px-3 py-2 text-[12px] font-light text-forensics-text">{browser}</div>}
            >
              <DenseDataTable
                rows={groupedRows}
                columns={columns}
                getRowKey={(row) => row.artifactId}
                emptyTitle={emptyTitle}
                emptyDescription=""
              />
            </DenseDataTableFrame>
          ))
      )}
    </TableBlock>
  );
}

export function BrowserHistoryPanel({
  summary,
  onRunBrowserPasswordExtraction,
}: {
  summary?: BrowserHistorySummary;
  onRunBrowserPasswordExtraction?: (password: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [passwordDialogOpen, setPasswordDialogOpen] = useState(false);
  const [password, setPassword] = useState('');
  const [passwordSubmitting, setPasswordSubmitting] = useState(false);
  const { visitColumns, downloadColumns, cookieColumns, sessionColumns, passwordColumns, extensionColumns } =
    browserColumns(t);
  const info = summary ?? {
    status: 'unavailable' as const,
    visitTotal: 0,
    downloadTotal: 0,
    cookieTotal: 0,
    sessionTotal: 0,
    passwordTotal: 0,
    extensionTotal: 0,
    visits: [],
    downloads: [],
    cookies: [],
    sessions: [],
    passwords: [],
    extensions: [],
    generatedAt: '',
    warnings: [t('browser.unavailable')],
  };

  const visitGroups = groupByBrowser(info.visits);
  const extensionRows = info.extensions ?? [];

  async function submitPassword() {
    if (!password || passwordSubmitting || !onRunBrowserPasswordExtraction) return;
    setPasswordSubmitting(true);
    try {
      await onRunBrowserPasswordExtraction(password);
      setPassword('');
      setPasswordDialogOpen(false);
    } finally {
      setPasswordSubmitting(false);
    }
  }

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
        [t('browser.stats.extensions'), (info.extensionTotal ?? extensionRows.length).toString()],
        [t('browser.stats.browsers'), Object.keys(visitGroups).sort(browserOrder).join(' / ') || '-'],
      ]}
    >
      {onRunBrowserPasswordExtraction ? (
        <>
          <div className="mb-4 flex items-center justify-between gap-3 border-b border-forensics-divider pb-3">
            <div className="text-[11px] text-forensics-muted">{t('browser.manualPasswordHint')}</div>
            <IconButton
              label={t('browser.manualPasswordAction')}
              variant="forensicsOutline"
              onClick={() => setPasswordDialogOpen(true)}
            >
              <ActionIcon name="key" />
            </IconButton>
          </div>
          <Dialog open={passwordDialogOpen} onOpenChange={setPasswordDialogOpen}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>{t('browser.manualPasswordTitle')}</DialogTitle>
                <DialogDescription>{t('browser.manualPasswordDescription')}</DialogDescription>
              </DialogHeader>
              <Input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={t('browser.manualPasswordPlaceholder')}
                autoFocus
              />
              <DialogFooter>
                <IconButton
                  label={t('browser.manualPasswordSubmit')}
                  variant="forensicsPrimary"
                  disabled={!password || passwordSubmitting}
                  onClick={() => void submitPassword()}
                >
                  <ActionIcon name="unlock" />
                </IconButton>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </>
      ) : null}
      <div className="space-y-4">
        <BrowserTableSection
          title={t('browser.sections.visits')}
          rows={info.visits}
          columns={visitColumns}
          emptyTitle={t('browser.empty.visits')}
          emptyDescription={t('browser.empty.visitsDescription')}
        />

        <BrowserTableSection
          title={t('browser.sections.downloads')}
          rows={info.downloads}
          columns={downloadColumns}
          emptyTitle={t('browser.empty.downloads')}
          emptyDescription={t('browser.empty.downloadsDescription')}
        />

        <BrowserTableSection
          title={t('browser.sections.cookies')}
          rows={info.cookies}
          columns={cookieColumns}
          emptyTitle={t('browser.empty.cookies')}
          emptyDescription={t('browser.empty.cookiesDescription')}
        />

        <BrowserTableSection
          title={t('browser.sections.sessions')}
          rows={info.sessions}
          columns={sessionColumns}
          emptyTitle={t('browser.empty.sessions')}
          emptyDescription={t('browser.empty.sessionsDescription')}
        />

        <BrowserTableSection
          title={t('browser.sections.passwords')}
          rows={info.passwords}
          columns={passwordColumns}
          emptyTitle={t('browser.empty.passwords')}
          emptyDescription={t('browser.empty.passwordsDescription')}
        />

        <BrowserTableSection
          title={t('browser.sections.extensions')}
          rows={extensionRows}
          columns={extensionColumns}
          emptyTitle={t('browser.empty.extensions')}
          emptyDescription={t('browser.empty.extensionsDescription')}
        />
      </div>
    </ExtractionTableSection>
  );
}
