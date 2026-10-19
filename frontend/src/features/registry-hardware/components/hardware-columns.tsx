import type { TFunction } from 'i18next';
import type { DenseColumn } from '@/components/tables/DenseDataTable';
import type { HardwareRow } from '../hardware-model';

export function hardwareColumns(t: TFunction): DenseColumn<HardwareRow>[] {
  return [
    { key: 'name', title: t('registryHardware.columns.device'), className: 'registry-hardware-name-column',
      render: (row) => <div className="registry-hardware-device"><span>{row.name}</span><small>{row.manufacturer}</small></div> },
    { key: 'details', title: t('registryHardware.columns.specification'), className: 'registry-hardware-details-column',
      render: (row) => <div className="registry-hardware-specs">{row.details.length
        ? row.details.map((detail) => <span key={detail.label}><small>{detail.label}</small>{detail.value}</span>)
        : <span>{t('registryHardware.notExtracted')}</span>}</div> },
    { key: 'source', title: t('registryHardware.columns.source'), className: 'registry-hardware-source-column',
      render: (row) => <div className="registry-hardware-origin"><code>{row.source}</code>{row.offset ? <small>{row.offset}</small> : null}</div> },
  ];
}
