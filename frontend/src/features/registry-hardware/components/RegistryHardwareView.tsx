import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import type { RegistryHardwareModel } from '../use-registry-hardware-model';
import { hardwareColumns } from './hardware-columns';

export function RegistryHardwareView({ model }: { model: RegistryHardwareModel }) {
  const { t } = model;
  const columns = hardwareColumns(t);
  return (
    <section className="registry-hardware registry-subview" aria-label={t('registryHardware.title')}>
      <p className="registry-hardware-note">{t('registryHardware.description')}</p>
      {model.memory.capacity ? <p className="registry-hardware-total">{model.memory.capacity}</p> : null}
      {model.memory.unknown ? <p className="registry-hardware-note">{model.memory.unknown}</p> : null}
      {model.warnings.length ? (
        <p className="registry-hardware-note" role="status">
          {t('registryHardware.partial')}
        </p>
      ) : null}
      {model.groups.length ? (
        model.groups.map((group) => (
          <section key={group.kind} className="registry-hardware-group">
            <h3>
              <group.icon aria-hidden="true" />
              {group.title}
              <small>{t('registryHardware.recordCount', { count: group.rows.length })}</small>
            </h3>
            <DenseDataTableFrame rowCount={group.rows.length}>
              <DenseDataTable
                rows={group.rows}
                columns={columns}
                getRowKey={(row) => row.id}
                horizontalScroll
                minColumnWidth={200}
                emptyTitle={t('registryHardware.empty', { kind: group.title })}
                emptyDescription={t('registryHardware.emptyDescription')}
              />
            </DenseDataTableFrame>
          </section>
        ))
      ) : (
        <p className="registry-browser-message">{t('registryHardware.emptyDescription')}</p>
      )}
    </section>
  );
}
