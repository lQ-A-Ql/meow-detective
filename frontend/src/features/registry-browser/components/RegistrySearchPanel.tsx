import { useTranslation } from 'react-i18next';
import { Input } from '@/app/components/ui/input';
import { Button } from '@/app/components/ui/button';
import { EvidenceTaskStatus } from '@/features/jobs/components/EvidenceTaskStatus';
import type { useRegistrySearch } from '../hooks/use-registry-search';
import type { RegistrySearchHit } from '@/types/registryBrowser';
export function RegistrySearchPanel({ model, enabled, onLocate }: { model: ReturnType<typeof useRegistrySearch>; enabled: boolean; onLocate: (hit: RegistrySearchHit) => void }) {
  const { t } = useTranslation();
  return <section className="space-y-2 border-b border-forensics-border p-2" aria-label={t('registryBrowser.search.title')}>
    <form className="flex gap-2" onSubmit={(event) => { event.preventDefault(); model.run(); }}>
      <Input aria-label={t('registryBrowser.search.query')} value={model.query} onChange={(event) => model.setQuery(event.target.value)} disabled={!enabled || model.loading} />
      <Button shape="row" type="submit" variant="forensicsOutline" disabled={!enabled || model.loading || !model.query.trim()}>{t('registryBrowser.search.run')}</Button>
    </form>
    <EvidenceTaskStatus model={model} />
    {model.page ? <div className="space-y-1 text-[11px]" aria-live="polite"><div>{t('registryBrowser.search.summary', { keys: model.page.scannedKeys, values: model.page.scannedValues, hits: model.page.totalMatches ?? model.page.hits.length, skipped: model.page.skippedCount })}</div>
      {model.page.truncated ? <div>{t('registryBrowser.search.truncated')}</div> : null}
      {model.page.complete === false || model.page.skippedCount > 0 ? <div className="text-forensics-warning-text">{t('registryBrowser.search.partial')}</div> : null}
      <div className="max-h-40 overflow-auto">{model.page.hits.map((hit, index) => <Button key={`${hit.kind}:${hit.cellOffset}:${index}`} shape="row" variant="forensicsGhost" onClick={() => onLocate(hit)}><span className="block truncate font-mono" title={hit.preview}>{hit.path} · {hit.name || t('registryBrowser.defaultValue')} · {t(`registryBrowser.search.fields.${hit.matchedField}`)} · 0x{hit.cellOffset.toString(16)}</span></Button>)}</div>
    </div> : null}
  </section>;
}
