import { ScrollArea } from '@/app/components/ui/scroll-area';
import { PanelTabs, TabsContent } from '@/components/tabs/PanelTabs';
import { HexViewer } from '@/components/viewers/HexViewer';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import type { RegistryBrowserModel } from '../use-registry-browser-model';
import type { RegistryDetailTab } from '../types';

export function RegistryValueDetail({ model }: { model: RegistryBrowserModel }) {
  const { t } = model;
  const range = model.range.data;
  return <section className="registry-browser-pane" aria-label={t('registryBrowser.detailsTitle')}>
    <div className="registry-browser-pane-heading"><span>{model.selectedValueLabel}</span>
      <IconButton label={t('registryBrowser.copyValue')} disabled={range?.decodedText === undefined} onClick={model.copyValue}><ActionIcon name="copy" /></IconButton></div>
    {!model.selectedValue ? <p className="registry-browser-message">{t('registryBrowser.selectValue')}</p> : <>
      <PanelTabs value={model.detailTab} onValueChange={(value) => model.setDetailTab(value as RegistryDetailTab)}
        tabs={['decoded', 'hex', 'provenance'].map((value) => ({ value, label: t(`registryBrowser.${value}`) }))} variant="underline" className="min-h-0 flex-1">
        <TabsContent value="decoded" className="registry-browser-detail-content">
          <ScrollArea className="min-h-0 flex-1"><div className="registry-browser-data">
            {model.range.isPending ? <p>{t('registryBrowser.loading')}</p> : null}
            {model.rangeError ? <p role="alert">{model.rangeError}</p> : null}
            {range?.decodeStatus === 'partial' ? <p>{t('registryBrowser.partialRange')}</p> : null}
            {range?.decodedText !== undefined ? <pre>{range.decodedText || t('registryBrowser.emptyString')}</pre>
              : range ? <p>{t('registryBrowser.rawOnly')}</p> : null}
          </div></ScrollArea>
        </TabsContent>
        <TabsContent value="hex" className="registry-browser-detail-content">
          {range ? <HexViewer lines={[]} rawBytes={range.bytes} baseOffset={range.offset} fileSize={range.totalLength} /> : null}
        </TabsContent>
        <TabsContent value="provenance" className="registry-browser-detail-content">
          <ScrollArea className="min-h-0 flex-1"><dl className="registry-browser-provenance">
            {model.provenance.map(({ name, value }) => <div key={name}><dt>{name}</dt><dd>{value}</dd></div>)}
          </dl></ScrollArea>
        </TabsContent>
      </PanelTabs>
      <div className="registry-browser-range">
        <span>{model.rangeSummary}</span>
        <IconButton label={t('registryBrowser.previousRange')} disabled={model.offset === 0 || model.range.isFetching} onClick={model.previousRange}><ActionIcon name="chevron-left" /></IconButton>
        <IconButton label={t('registryBrowser.nextRange')} disabled={!range || range.eof || model.range.isFetching} onClick={model.nextRange}><ActionIcon name="chevron-right" /></IconButton>
      </div>
    </>}
  </section>;
}
