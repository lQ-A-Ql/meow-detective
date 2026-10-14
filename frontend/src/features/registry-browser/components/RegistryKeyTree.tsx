import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Tree, TreeRow } from '@/app/components/ui/tree';
import { Button } from '@/app/components/ui/button';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';
import type { RegistryBrowserModel } from '../use-registry-browser-model';
import type { RegistryTreeNode } from '../types';

export function RegistryKeyTree({ model }: { model: RegistryBrowserModel }) {
  const { t } = model;
  return <aside className="registry-browser-pane">
    <div className="registry-browser-pane-heading">{t('registryBrowser.treeTitle')}</div>
    <div className="registry-browser-tree-search"><Input aria-label={t('registryBrowser.filter')}
      placeholder={t('registryBrowser.filter')} value={model.filter} onChange={(event) => model.setFilter(event.target.value)} />
      <small>{t('registryBrowser.filterHint')}</small></div>
    <div className="registry-browser-tree-search"><Input aria-label={t('registryBrowser.searchHive')} placeholder={t('registryBrowser.searchHive')} value={model.searchQuery} onChange={(event) => model.setSearchQuery(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') void model.search(); }} />
      {model.searchPage ? <small>{t('registryBrowser.searchSummary', { keys: model.searchPage.scannedKeys, values: model.searchPage.scannedValues, hits: model.searchPage.hits.length })}</small> : null}</div>
    {model.searchPage?.hits.length ? <div className="registry-browser-tree-search max-h-40 overflow-auto">
      {model.searchPage.hits.map((hit) => <button key={`${hit.kind}:${hit.cellOffset}`} type="button" className="block w-full truncate text-left text-[10px]" onClick={() => void model.locate(hit.path)}>
        <span className="font-mono">{hit.path}\{hit.name}</span><span className="ml-1 text-forensics-muted">{hit.matchedField}</span>
      </button>)}
    </div> : null}
    <ScrollArea className="min-h-0 flex-1" viewportRef={model.viewport.viewportRef} viewportClassName="registry-browser-tree-scroll">
      {model.hives.isPending ? <p className="registry-browser-message">{t('registryBrowser.loading')}</p> : null}
      {model.hivesError ? <div className="registry-browser-message" role="alert">{model.hivesError}
        <Button shape="row" onClick={() => void model.hives.refetch()}>{t('registryBrowser.retry')}</Button></div> : null}
      {!model.hives.isPending && !model.hivesError && !model.nodes.length
        ? <p className="registry-browser-message">{t(model.hives.data?.hives.length ? 'registryBrowser.noMatches' : 'registryBrowser.emptySource')}</p> : null}
      <Tree ref={model.keyboard.treeRef} className="relative" style={{ height: model.viewport.virtualizer.getTotalSize() }}
        aria-label={t('registryBrowser.treeTitle')} onKeyDown={model.keyboard.onKeyDown}>
        {model.viewport.virtualizer.getVirtualItems().map((row) => <div key={row.key} data-index={row.index}
          ref={model.viewport.virtualizer.measureElement} className="absolute left-0 top-0 w-full" style={{ transform: `translateY(${row.start}px)` }}>
          <RegistryTreeNodeView node={model.viewport.rows[row.index]} model={model} />
        </div>)}
      </Tree>
      {model.hives.hasNextPage ? <Button shape="row" disabled={model.hives.isFetchingNextPage}
        onClick={() => void model.hives.fetchNextPage()}>{t('registryBrowser.loadMore')}</Button> : null}
    </ScrollArea>
  </aside>;
}

function RegistryTreeNodeView({ node, model }: { node: RegistryTreeNode; model: RegistryBrowserModel }) {
  const { t } = model;
  const selected = node.key?.keyId === model.selectedKey?.keyId && node.hive.fileId === model.session?.hive.fileId;
  return <div>
    <TreeRow id={node.domId} depth={node.depth} label={node.label} ariaLabel={node.label}
      active={selected} selected={selected} expanded={node.expanded} hasChildren={node.hasChildren}
      tabIndex={model.keyboard.focusedId === node.id ? 0 : -1} compositeNavigation
      positionInSet={node.position} setSize={node.setSize}
      onFocus={() => model.keyboard.setFocusedId(node.id)} onActivate={() => model.activate(node)}
      onToggle={() => model.toggle(node)} toggleLabel={t(node.expanded ? 'registryBrowser.collapse' : 'registryBrowser.expand', { name: node.label })}
      icon={<ActionIcon name={node.hive.readState === 'locked' ? 'lock' : node.expanded ? 'folder-open' : 'folder'} />}
      style={{ paddingLeft: `${REGISTRY_BROWSER_LAYOUT.treePaddingPixels + node.depth * REGISTRY_BROWSER_LAYOUT.treeIndentPixels}px` }}
      meta={node.key?.readState === 'corrupt' ? <ActionIcon name="warning" /> : null} />
    {node.loading ? <p className="registry-browser-tree-note">{t('registryBrowser.loading')}</p> : null}
    {node.error ? <div className="registry-browser-tree-note" role="alert">{t(`registryBrowser.errors.${node.error}`)}
      <Button shape="row" onClick={() => model.loadChildren(node)}>{t('registryBrowser.retry')}</Button></div> : null}
    {node.nextCursor ? <Button shape="row" onClick={() => model.loadChildren(node)}>{t('registryBrowser.loadMore')}</Button> : null}
  </div>;
}
