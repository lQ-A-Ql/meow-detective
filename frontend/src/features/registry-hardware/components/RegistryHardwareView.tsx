import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { RegistryHiveView } from '@/types/registryBrowser';
import type { RegistryHardwareModel } from '../use-registry-hardware-model';
import { hardwareColumns } from './hardware-columns';

export function RegistryHardwareView({ model }: { model: RegistryHardwareModel }) {
  const { t } = model;
  const columns = hardwareColumns(t);
  return <section className="registry-hardware registry-subview" aria-label={t('registryHardware.title')}>
    <div className="registry-hardware-toolbar">
      {model.systems.length ? <Select value={model.selected?.fileId} disabled={model.loading} onValueChange={model.selectHive}>
        <SelectTrigger aria-label={t('registryHardware.hiveLabel')}><SelectValue /></SelectTrigger>
        <SelectContent>{model.systems.map((hive) => <SelectItem key={hive.fileId} value={hive.fileId}>{hive.evidencePath}</SelectItem>)}</SelectContent>
      </Select> : null}
      <Select value={model.view} disabled={model.loading} onValueChange={(value) => model.setView(value as RegistryHiveView)}>
        <SelectTrigger aria-label={t('registryBrowser.viewLabel')}><SelectValue /></SelectTrigger>
        <SelectContent><SelectItem value="base">{t('registryBrowser.baseView')}</SelectItem><SelectItem value="recovered">{t('registryBrowser.recoveredView')}</SelectItem></SelectContent>
      </Select>
      <IconButton label={t('registryHardware.refresh')} disabled={model.loading} onClick={model.retry}><ActionIcon name="refresh" /></IconButton>
      {model.hasMore ? <IconButton label={t('registryHardware.loadMore')} disabled={model.loadingMore} onClick={model.loadMore}><ActionIcon name="chevron-down" /></IconButton> : null}
    </div>
    <p className="registry-hardware-note">{t('registryHardware.description')}</p>
    {model.selected ? <code className="registry-hardware-path">{model.selected.evidencePath}</code> : null}
    {model.errorText ? <p className="registry-browser-error" role="alert">{model.errorText}</p>
      : model.loading ? <p className="registry-browser-message" role="status">{t('registryHardware.loading')}</p>
        : !model.selected ? <p className="registry-browser-message">{t('registryHardware.noHive')}</p>
          : <>
            {model.memory.capacity ? <p className="registry-hardware-total">{model.memory.capacity}</p> : null}
            {model.memory.unknown ? <p className="registry-hardware-note">{model.memory.unknown}</p> : null}
            {model.warnings.length ? <p className="registry-hardware-note" role="status">{t('registryHardware.partial')}</p> : null}
            {model.recovery ? <p className="registry-hardware-note">{t('registryHardware.recovery', { status: t(`registryBrowser.recoveryStatuses.${model.recovery.status}`) })}</p> : null}
            {model.groups.map((group) => <section key={group.kind} className="registry-hardware-group">
              <h3><group.icon aria-hidden="true" />{group.title}<small>{t('registryHardware.recordCount', { count: group.rows.length })}</small></h3>
              <DenseDataTableFrame rowCount={group.rows.length}>
                <DenseDataTable rows={group.rows} columns={columns} getRowKey={(row) => row.id} horizontalScroll minColumnWidth={200}
                  emptyTitle={t('registryHardware.empty', { kind: group.title })} emptyDescription={t('registryHardware.emptyDescription')} />
              </DenseDataTableFrame>
            </section>)}
          </>}
  </section>;
}
