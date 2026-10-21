import { Label } from '@/app/components/ui/label';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ResizableGroup, ResizableHandle, ResizablePanel } from '@/app/components/ui/resizable';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { REGISTRY_BROWSER_LAYOUT as layout } from '@/design/registry-browser';
import type { RegistryBrowserModel } from '../use-registry-browser-model';
import { RegistryKeyTree } from './RegistryKeyTree';
import { RegistryValuesPane } from './RegistryValuesPane';
import { RegistryValueDetail } from './RegistryValueDetail';
import { RegistryRecoverySummary } from './RegistryRecoverySummary';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import type { RegistryHiveView } from '@/types/registryBrowser';
import { RegistrySearchPanel } from './RegistrySearchPanel';

export function RegistryBrowserView({ model }: { model: RegistryBrowserModel }) {
  const { t } = model;
  return <section className="registry-browser registry-subview" aria-label={t('registryBrowser.title')}>
    <header className="registry-browser-header">
      <div><h1>{t('registryBrowser.title')}</h1><p>{t('registryBrowser.rawTab')}</p></div>
      <div className="registry-browser-badges">
        <Select value={model.view} disabled={model.opening} onValueChange={(view) => model.switchView(view as RegistryHiveView)}>
          <SelectTrigger aria-label={t('registryBrowser.viewLabel')}><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="base">{t('registryBrowser.baseView')}</SelectItem><SelectItem value="recovered">{t('registryBrowser.recoveredView')}</SelectItem></SelectContent>
        </Select>
        <span><ActionIcon name="lock" />{t('registryBrowser.readOnly')}</span>
      </div>
    </header>
    <div className="registry-browser-source">
      <span>{model.session?.hive.profileLabel ?? model.session?.hive.name ?? model.context.dataSourceId}</span>
      <code>{model.session?.hive.evidencePath}</code>
      {model.session?.diagnostics.map((item) => <span key={item.code}>{t(`registryBrowser.diagnostics.${item.code}`)}</span>)}
    </div>
    <RegistryRecoverySummary model={model} />
    <RegistrySearchPanel model={model.registrySearch} enabled={Boolean(model.session) && !model.opening} onLocate={(hit) => void model.locateSearchHit(hit)} />
    <form className="registry-browser-address" onSubmit={(event) => { event.preventDefault(); void model.locate(model.pathInput); }}>
      <Label htmlFor={`${model.instanceId}-path`}>{t('registryBrowser.path')}</Label>
      <Input id={`${model.instanceId}-path`} value={model.pathInput} disabled={!model.session}
        onChange={(event) => model.setPathInput(event.target.value)} placeholder={t('registryBrowser.path')} />
      <IconButton type="submit" label={t('registryBrowser.go')} disabled={!model.session || model.locating}><ActionIcon name="chevron-right" /></IconButton>
      <IconButton label={t('registryBrowser.copyPath')} disabled={!model.selectedKey} onClick={model.copyPath}><ActionIcon name="copy" /></IconButton>
    </form>
    {model.pathError ? <p className="registry-browser-error" role="alert">{model.pathError}</p> : null}
    {model.hiveError ? <div className="registry-browser-error" role="alert">{model.hiveError}
      <Button shape="row" onClick={() => void model.retryHive()}>{t('registryBrowser.retry')}</Button></div> : null}
    <ResizableGroup direction="horizontal">
      <ResizablePanel defaultSize={layout.treeDefaultPercent} minSize={layout.treeMinPercent} maxSize={layout.treeMaxPercent}>
        <RegistryKeyTree model={model} />
      </ResizablePanel>
      <ResizableHandle aria-label={t('registryBrowser.resizeTree')} />
      <ResizablePanel defaultSize={100 - layout.treeDefaultPercent} minSize={40}>
        <ResizableGroup direction="vertical">
          <ResizablePanel defaultSize={layout.valuesDefaultPercent} minSize={25}><RegistryValuesPane model={model} /></ResizablePanel>
          <ResizableHandle aria-label={t('registryBrowser.resizeDetails')} />
          <ResizablePanel defaultSize={100 - layout.valuesDefaultPercent} minSize={layout.detailsMinPercent}><RegistryValueDetail model={model} /></ResizablePanel>
        </ResizableGroup>
      </ResizablePanel>
    </ResizableGroup>
    <footer className="registry-browser-footer" aria-live="polite">
      <span>{model.opening || model.locating ? t('registryBrowser.loading') : model.selectedKey?.path || model.session?.hive.name}</span>
      <span>{model.copyStatus}</span>
    </footer>
  </section>;
}
