import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Clock, Database, HardDrive, Shield, Usb, Wifi } from 'lucide-react';
import type { RegistryExtractionSummary, RegistryHiveOverview, RegistryStructuredSummary } from '@/types/models';
import type { DenseColumn } from '@/components/tables/DenseDataTable';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { ExtractionTableSection } from './helpers';
import { createRegistryColumns } from './registry-columns';

const TABLE_CONTENT_CLASS = 'm-0 flex-col overflow-hidden data-[state=active]:flex data-[state=inactive]:hidden';
type RegistryTab = 'users' | 'activity' | 'network' | 'software' | 'usb' | 'raw';

export function RegistryExtractionPanel({ summary, structured }: { summary?: RegistryExtractionSummary; structured?: RegistryStructuredSummary }) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<RegistryTab>('users');
  const columns = useMemo(() => createRegistryColumns(t), [t]);
  const info = summary ?? { status: 'unavailable' as const, total: 0, values: [], generatedAt: '', warnings: [t('analysis.registry.unavailable')] };
  const data = structured;
  const hiveOverviews: RegistryHiveOverview[] = data?.hiveOverviews ?? [];
  const tabs = [
    { key: 'users' as const, label: t('analysis.registry.tabs.users'), icon: Shield },
    { key: 'activity' as const, label: t('analysis.registry.tabs.activity'), icon: Clock },
    { key: 'network' as const, label: t('analysis.registry.tabs.network'), icon: Wifi },
    { key: 'software' as const, label: t('analysis.registry.tabs.software'), icon: Database },
    { key: 'usb' as const, label: t('analysis.registry.tabs.usb'), icon: Usb },
    { key: 'raw' as const, label: t('analysis.registry.tabs.raw'), icon: HardDrive },
  ];
  const stats: Array<[string, string]> = hiveOverviews.length > 0
    ? hiveOverviews.map((hive) => [hive.hiveName, `${hive.keyValueCount} ${t('analysis.registry.values.entries')}${hive.txlogMerged ? ` · ${t('analysis.registry.values.txlog')}` : ''}${hive.deletedKeysFound > 0 ? ` · ${t('analysis.registry.values.deleted', { count: hive.deletedKeysFound })}` : ''}`])
    : [[t('analysis.registry.values.keyValues'), info.total.toString()], [t('analysis.registry.values.sourceHives'), new Set(info.values.map((value) => value.hivePath)).size.toString()]];
  const empty = (key: string) => ({ emptyTitle: t(`analysis.registry.empty.${key}Title`), emptyDescription: t(`analysis.registry.empty.${key}Description`) });

  return (
    <ExtractionTableSection title={t('analysis.registry.title')} status={info.status} generatedAt={info.generatedAt} warnings={info.warnings} stats={stats}>
      <PanelTabs value={activeTab} onValueChange={(value) => setActiveTab(value as RegistryTab)} tabs={tabs.map(({ key, label, icon }) => ({ value: key, label, icon }))} variant="underline">
        <TabsContent value="users" className={TABLE_CONTENT_CLASS}>
          <RegistryTable rows={data?.samUsers ?? []} columns={columns.samColumns} rowKey={(row) => row.username} {...empty('users')} />
        </TabsContent>
        <TabsContent value="activity" className={TABLE_CONTENT_CLASS}>
          <RegistryTable rows={data?.userAssistEntries ?? []} columns={columns.userAssistColumns} rowKey={(row) => row.programPath} {...empty('activity')} />
        </TabsContent>
        <TabsContent value="network" className={TABLE_CONTENT_CLASS}>
          <div className="space-y-3">
            <RegistryTable title={t('analysis.registry.network.adapters')} rows={data?.networkAdapters ?? []} columns={columns.adapterColumns} rowKey={(row) => row.guid} {...empty('adapters')} />
            <RegistryTable title={t('analysis.registry.network.profiles')} rows={data?.networkProfiles ?? []} columns={columns.networkColumns} rowKey={(row) => row.profileGuid} {...empty('profiles')} />
          </div>
        </TabsContent>
        <TabsContent value="software" className={TABLE_CONTENT_CLASS}>
          <RegistryTable rows={data?.installedSoftware ?? []} columns={columns.softwareColumns} rowKey={(row) => `${row.displayName}-${row.version}`} {...empty('software')} />
        </TabsContent>
        <TabsContent value="usb" className={TABLE_CONTENT_CLASS}>
          <RegistryTable rows={data?.usbDevices ?? []} columns={columns.usbColumns} rowKey={(row) => row.serialNumber} {...empty('usb')} />
        </TabsContent>
        <TabsContent value="raw" className={TABLE_CONTENT_CLASS}>
          <RegistryTable rows={info.values} columns={columns.rawColumns} rowKey={(row) => row.artifactId} {...empty('raw')} />
        </TabsContent>
      </PanelTabs>
    </ExtractionTableSection>
  );
}

function RegistryTable<T>({ title, rows, columns, rowKey, emptyTitle, emptyDescription }: { title?: string; rows: T[]; columns: DenseColumn<T>[]; rowKey: (row: T) => string; emptyTitle: string; emptyDescription: string }) {
  return (
    <section>
      {title ? <div className="mb-2 text-[12px] font-light text-forensics-text">{title}</div> : null}
      <DenseDataTableFrame rowCount={rows.length}>
        <DenseDataTable rows={rows} columns={columns} getRowKey={rowKey} emptyTitle={emptyTitle} emptyDescription={emptyDescription} />
      </DenseDataTableFrame>
    </section>
  );
}
