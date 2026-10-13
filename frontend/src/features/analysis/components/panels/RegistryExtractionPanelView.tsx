import type { RegistryPanelModel } from '../../hooks/use-registry-panel-model';
import type { DenseColumn } from '@/components/tables/DenseDataTable';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { ExtractionTableSection } from './helpers';
import { NativeRegistryBrowserContainer } from '@/features/registry-browser/containers/NativeRegistryBrowserContainer';
import { createRegistryColumns } from './registry-columns';

const TABLE_CONTENT_CLASS = 'm-0 flex-col overflow-hidden data-[state=active]:flex data-[state=inactive]:hidden';

export function RegistryExtractionPanelView({ model, dataSourceId }: { model: RegistryPanelModel; dataSourceId?: string }) {
  const { t, activeTab, setActiveTab, info, data, tabs, stats, empty } = model;
  const columns = createRegistryColumns(t);
  return (
    <div className="registry-extraction-workspace">
      {activeTab !== 'raw' ? <ExtractionTableSection title={t('analysis.registry.title')} status={info.status} generatedAt={info.generatedAt} warnings={info.warnings} stats={stats}>{null}</ExtractionTableSection> : null}
      <PanelTabs className="min-h-0 flex-1" value={activeTab} onValueChange={(value) => setActiveTab(value as typeof activeTab)} tabs={tabs.map(({ key, label, icon }) => ({ value: key, label, icon }))} variant="underline">
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
        <TabsContent value="raw" data-registry-raw className={`${TABLE_CONTENT_CLASS} min-h-0 flex-1`}>
          {activeTab === 'raw' ? dataSourceId ? <NativeRegistryBrowserContainer dataSourceId={dataSourceId} />
            : <p className="registry-browser-message">{t('registryBrowser.errors.sourceUnavailable')}</p> : null}
        </TabsContent>
      </PanelTabs>
    </div>
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
