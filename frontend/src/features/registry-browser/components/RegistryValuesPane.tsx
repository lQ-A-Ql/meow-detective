import { DenseDataTable, type DenseColumn } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { RegistryBrowserModel } from '../use-registry-browser-model';
import type { RegistryValueRow } from '../types';

export function RegistryValuesPane({ model }: { model: RegistryBrowserModel }) {
  const { t } = model;
  const columns: DenseColumn<RegistryValueRow>[] = [
    { key: 'name', title: t('registryBrowser.name'), render: (value) => value.displayName, className: 'w-[160px]' },
    { key: 'type', title: t('registryBrowser.type'), render: (value) => value.typeName, className: 'w-[150px] font-mono' },
    { key: 'data', title: t('registryBrowser.data'), render: (value) => value.displayPreview, className: 'min-w-[240px] font-mono' },
    { key: 'length', title: t('registryBrowser.bytes'), render: (value) => value.displayLength, className: 'w-[90px] font-mono' },
  ];
  return <section className="registry-browser-pane" aria-label={t('registryBrowser.valuesTitle')}>
    <div className="registry-browser-pane-heading"><span>{t('registryBrowser.valuesTitle')}</span>
      {model.selectedKey ? <small>{t('registryBrowser.loaded', { loaded: model.loadedCount, total: model.declaredCount })}</small> : null}</div>
    {model.partial ? <p className="registry-browser-notice" role="status">{t('registryBrowser.partial')}</p> : null}
    {model.session && model.values.isPending ? <p className="registry-browser-message">{t('registryBrowser.loading')}</p> : null}
    <DenseDataTableFrame layout="fill" variant="plain" className="registry-browser-values">
      <DenseDataTable rows={model.rows} columns={columns} getRowKey={(value) => value.valueId}
        selectedRowKey={model.selectedValue?.valueId} onRowClick={(value) => model.selectValue(value.valueId)}
        emptyTitle={t(!model.session ? 'registryBrowser.selectHive' : model.values.isPending ? 'registryBrowser.loading' : 'registryBrowser.emptyKey')}
        emptyDescription={model.session && model.values.isPending ? '' : t(model.session ? 'registryBrowser.emptyKeyDescription' : 'registryBrowser.emptySourceDescription')}
        initialLoadFailed={Boolean(model.valuesError)} initialLoadErrorText={model.valuesError}
        onRetryInitialLoad={() => void model.values.refetch()}
        loadContextKey={`${model.session?.sessionId}:${model.selectedKey?.keyId}`}
        hasMore={model.values.hasNextPage} loadingMore={model.values.isFetchingNextPage}
        onReachEnd={() => void model.values.fetchNextPage()} horizontalScroll />
    </DenseDataTableFrame>
  </section>;
}
