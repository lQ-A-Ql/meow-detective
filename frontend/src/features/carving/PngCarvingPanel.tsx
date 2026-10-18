import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Input } from '@/app/components/ui/input';
import { Badge } from '@/app/components/ui/badge';
import { HexViewer } from '@/components/viewers/HexViewer';
import type { PngCarvingModel } from './hooks';
import { useTranslation } from 'react-i18next';
import { CollapsibleSection } from '@/components/layout/CollapsibleSection';
import { ScrollArea } from '@/app/components/ui/scroll-area';

export function PngCarvingPanel({ model }: { model: PngCarvingModel }) {
  const { t } = useTranslation();
  if (!model.supported) return null;
  return <CollapsibleSection title={t('carving.title')} defaultOpen={false} className="border border-forensics-border p-3" contentClassName="space-y-2 pt-2">
    <p className="text-[11px] text-forensics-muted">{t('carving.scope')}</p>
    <div className="flex flex-wrap gap-2">
      <Input aria-label={t('carving.offset')} placeholder={t('carving.offset')} value={model.offset} onChange={e => model.setOffset(e.target.value)} className="max-w-56" />
      <Input aria-label={t('carving.length')} placeholder={t('carving.length')} value={model.length} onChange={e => model.setLength(e.target.value)} className="max-w-80" />
      <IconButton label={t('carving.start')} variant="forensicsOutline" disabled={!model.valid || model.scanning} onClick={model.start}><ActionIcon name="play" /></IconButton>
      <IconButton label={t('carving.load')} onClick={model.load}><ActionIcon name="list" /></IconButton>
      {model.scanning ? <IconButton label={t('carving.cancel')} onClick={model.cancel}><ActionIcon name="close" /></IconButton> : null}
    </div>
    {!model.valid ? <p className="text-[11px] text-forensics-error-text">{t('carving.invalid')}</p> : null}
    {model.summary ? <div className="flex gap-2 text-[11px]">
      <Badge variant="outline">{t('carving.verified')}: {model.summary.complete}</Badge>
      <Badge variant="outline">{t('carving.rejected')}: {model.summary.rejected}</Badge>
      <Badge variant="outline">{t('carving.partial')}: {model.summary.partial}</Badge>
    </div> : null}
    <Input aria-label={t('carving.search')} placeholder={t('carving.search')} value={model.query} onChange={e => model.setQuery(e.target.value)} />
    <ScrollArea className="h-48" viewportClassName="space-y-1">
      {model.items.map(item => <div key={item.artifactId} className="flex flex-wrap items-center gap-2 text-[11px]">
        <span className="font-mono">{item.offset} + {item.length}</span>
        <span className="min-w-0 flex-1 truncate font-mono" title={item.sha256}>{item.sha256}</span>
        <IconButton label={t('carving.preview')} disabled={model.busy} onClick={() => model.read(item)}><ActionIcon name="eye" /></IconButton>
        <IconButton label={t('carving.export')} disabled={model.busy} onClick={() => model.extract(item)}><ActionIcon name="download" /></IconButton>
      </div>)}
    </ScrollArea>
    {model.hasPrevious || model.hasNext ? <div className="flex gap-2">
      <IconButton label={t('carving.first')} disabled={!model.hasPrevious} onClick={model.first}><ActionIcon name="arrow-left" /></IconButton>
      <IconButton label={t('carving.more')} disabled={!model.hasNext} onClick={model.next}><ActionIcon name="chevron-right" /></IconButton>
    </div> : null}
    {model.preview ? <div className="h-40"><HexViewer lines={[]} rawBytes={model.preview.bytes} /></div> : null}
    {model.exported ? <p className="break-all text-[11px]">{t('carving.exported')}: {model.exported.sha256}</p> : null}
    {model.error ? <p className="text-[11px] text-forensics-error-text">{model.error}</p> : null}
  </CollapsibleSection>;
}
