import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryKeyMetadata, RegistryHiveView } from '@/types/registryBrowser';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';
import { useRegistryHives } from './hooks/use-registry-hives';
import { useRegistrySession } from './hooks/use-registry-session';
import { useRegistryTree } from './hooks/use-registry-tree';
import { useRegistryValues } from './hooks/use-registry-values';
import { useRegistryValueRange } from './hooks/use-registry-range';
import { useRegistryTreeKeyboard } from './hooks/use-registry-tree-keyboard';
import { filterRegistryTree, flattenRegistryTree, registryTreeNodes } from './tree-model';
import { useRegistryTreeViewport } from './hooks/use-registry-tree-viewport';
import { useRegistrySessionCache } from './hooks/use-registry-session-cache';
import { registryBrowserErrorCode } from './errors';
import { registryFiletimeUtc, registryValueRows } from './value-model';
import { recoveryFailureDetails } from './recovery-model';
import type { RegistryBrowserContext, RegistryDetailTab, RegistryTreeNode } from './types';

export function useRegistryBrowserModel(client: RegistryBrowserClient, context: RegistryBrowserContext, initialKeyPath = '') {
  const { t } = useTranslation();
  const instanceId = useId();
  const hives = useRegistryHives(client, context);
  const [view, setView] = useState<RegistryHiveView>('base');
  const pendingViewPath = useRef<{ view: RegistryHiveView; fileId: string; path: string }>();
  const hive = useRegistrySession(client, { ...context, sourceRevision: hives.data?.sourceRevision ?? context.sourceRevision });
  const session = hive.session;
  useRegistrySessionCache(session?.sessionId);
  const tree = useRegistryTree(client, session);
  const [selection, setSelection] = useState<{ sessionId: string; key: RegistryKeyMetadata }>();
  const selectedKey = selection?.sessionId === session?.sessionId ? selection?.key : session?.rootKey;
  const [valueSelection, setValueSelection] = useState<{ sessionId: string; keyId: string; valueId: string }>();
  const values = useRegistryValues(client, session, selectedKey);
  const rawValues = useMemo(() => values.data?.pages.flatMap((page) => page.values) ?? [], [values.data]);
  const selectedValue = valueSelection && valueSelection.sessionId === session?.sessionId && valueSelection.keyId === selectedKey?.keyId
    ? rawValues.find((value) => value.valueId === valueSelection.valueId) : undefined;
  const [offset, setOffset] = useState(0);
  const range = useRegistryValueRange(client, session, selectedKey, selectedValue, offset);
  const expired = registryBrowserErrorCode(values.error) === 'expiredSession'
    || registryBrowserErrorCode(range.error) === 'expiredSession' || Object.values(tree.errors).includes('expiredSession');
  useEffect(() => { if (expired) hive.expire(); }, [expired, hive.expire]);
  const [detailTab, setDetailTab] = useState<RegistryDetailTab>('decoded');
  const [filter, setFilter] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [searchPage, setSearchPage] = useState<import('@/types/registryBrowser').RegistrySearchPage>();
  const [pathInput, setPathInput] = useState('');
  const [pathError, setPathError] = useState<unknown>();
  const [locating, setLocating] = useState(false);
  const [copyStatus, setCopyStatus] = useState<string>();
  const navigationEpoch = useRef(0);
  const activeSession = useRef(session?.sessionId);
  activeSession.current = session?.sessionId;
  useEffect(() => () => { navigationEpoch.current += 1; }, []);
  const selectKey = useCallback((key: RegistryKeyMetadata) => {
    if (!session) return;
    navigationEpoch.current += 1;
    setLocating(false);
    setSelection({ sessionId: session.sessionId, key }); setPathInput(key.path);
    setPathError(undefined); setValueSelection(undefined); setOffset(0); setCopyStatus(undefined);
  }, [session]);
  const openHive = useCallback(async (fileId: string, revision?: string, requestedView = view) => {
    if (pendingViewPath.current && (pendingViewPath.current.fileId !== fileId || pendingViewPath.current.view !== requestedView)) pendingViewPath.current = undefined;
    navigationEpoch.current += 1;
    setLocating(false); setCopyStatus(undefined);
    setSelection(undefined); setValueSelection(undefined); setOffset(0); setPathInput(''); setPathError(undefined);
    return hive.openHive(fileId, revision, requestedView);
  }, [hive.openHive, view]);
  const activate = (node: RegistryTreeNode) => {
    if (node.key && session?.hive.fileId === node.hive.fileId) selectKey(node.key);
    else void openHive(node.hive.fileId);
  };
  const toggle = (node: RegistryTreeNode) => {
    if (node.key && session?.hive.fileId === node.hive.fileId) tree.toggle(node.key);
    else void openHive(node.hive.fileId);
  };
  const nodes = filterRegistryTree(registryTreeNodes(hives.data?.hives ?? [], session, tree.pages,
    tree.expanded, tree.loading, tree.errors, instanceId), filter);
  const keyboard = useRegistryTreeKeyboard(nodes, activate, toggle);
  const viewport = useRegistryTreeViewport(flattenRegistryTree(nodes), keyboard);
  const locate = useCallback(async (path: string) => {
    if (!session) return;
    const token = ++navigationEpoch.current;
    const sessionId = session.sessionId;
    const parts = path.split('\\').filter(Boolean);
    setLocating(true); setPathError(undefined);
    try {
      const key = await client.resolveKey({ sessionId, keyPath: parts.join('\\') });
      for (let depth = 0; depth < parts.length; depth++) {
        const parent = depth === 0 ? session.rootKey
          : await client.resolveKey({ sessionId, keyPath: parts.slice(0, depth).join('\\') });
        if (token !== navigationEpoch.current || activeSession.current !== sessionId) return;
        const childPath = parts.slice(0, depth + 1).join('\\').toLowerCase();
        let page = await tree.load(parent);
        while (page?.nextCursor && !page.keys.some((child) => child.path.toLowerCase() === childPath)) {
          if (token !== navigationEpoch.current || activeSession.current !== sessionId) return;
          page = await tree.load(parent, page.nextCursor);
        }
        tree.expand(parent);
      }
      if (token !== navigationEpoch.current || activeSession.current !== sessionId) return;
      setSelection({ sessionId, key }); setPathInput(key.path); setValueSelection(undefined); setOffset(0);
      keyboard.setFocusedId(key.keyId);
    } catch (error) {
      if (token === navigationEpoch.current && activeSession.current === sessionId) setPathError(error);
    } finally { if (token === navigationEpoch.current) setLocating(false); }
  }, [client, session, tree.load, tree.expand, keyboard.setFocusedId]);
  const initialDone = useRef(false);
  useEffect(() => {
    if (!initialDone.current && hives.data?.hives.length) {
      initialDone.current = true; void openHive(hives.data.hives[0].fileId);
    }
  }, [hives.data, openHive]);
  const initialPathDone = useRef(false);
  useEffect(() => {
    const pending = pendingViewPath.current;
    if (session && pending?.view === session.source.view && pending.fileId === session.hive.fileId) {
      pendingViewPath.current = undefined;
      if (pending.path) void locate(pending.path);
    }
  }, [session?.sessionId]);
  useEffect(() => {
    if (session && initialKeyPath && !initialPathDone.current) {
      initialPathDone.current = true; void locate(initialKeyPath);
    }
  }, [session?.sessionId]);
  const copy = async (text: string) => {
    const token = navigationEpoch.current;
    try { await navigator.clipboard.writeText(text); if (token === navigationEpoch.current) setCopyStatus(t('registryBrowser.copied')); }
    catch { if (token === navigationEpoch.current) setCopyStatus(t('registryBrowser.copyFailed')); }
  };
  const search = useCallback(async () => {
    if (!session || !searchQuery.trim()) { setSearchPage(undefined); return; }
    setSearchPage(await client.searchHive({ sessionId: session.sessionId, query: searchQuery }));
  }, [client, searchQuery, session]);
  const errorText = (error: unknown) => error ? t(`registryBrowser.errors.${registryBrowserErrorCode(error)}`) : undefined;
  const pages = values.data?.pages ?? [];
  const keyTime = registryFiletimeUtc(selectedKey?.lastWriteFiletime) ?? t('registryBrowser.unknown');
  const selectedValueLabel = selectedValue
    ? registryValueRows([selectedValue], t)[0].displayName : t('registryBrowser.detailsTitle');
  const provenance = [
    [t('registryBrowser.viewLabel'), t(view === 'base' ? 'registryBrowser.baseView' : 'registryBrowser.recoveredView')],
    [t('registryBrowser.baseHash'), session?.recovery?.baseSha256],
    [t('registryBrowser.snapshotHash'), session?.recovery?.resultSha256],
    [t('registryBrowser.fileId'), session?.source.fileId],
    [t('registryBrowser.sourcePath'), session?.hive.evidencePath],
    [t('registryBrowser.keyPath'), selectedKey?.path || '\\'],
    [t('registryBrowser.type'), selectedValue ? `${selectedValue.typeName} (${selectedValue.typeCode})` : undefined],
    [t('registryBrowser.cellOffset'), selectedValue ? `0x${selectedValue.cellOffset.toString(16)}` : undefined],
    [t('registryBrowser.lastWrite'), keyTime],
    [t('registryBrowser.filetime'), selectedKey?.lastWriteFiletime],
  ].map(([name, value]) => ({ name: name!, value: value ?? t('registryBrowser.unknown') }));
  const rangeSummary = range.data ? t('registryBrowser.range', {
    offset: range.data.offset.toLocaleString(), loaded: range.data.bytes.length.toLocaleString(), total: range.data.totalLength.toLocaleString(),
  }) : t('registryBrowser.previewOnly');
  return {
    t, instanceId, context, hives, session, selectedKey, selectedValue, range, values,
    searchQuery, setSearchQuery, searchPage, search,
    nodes, keyboard, viewport, activate, toggle, openHive, selectKey, filter, setFilter,
    pathInput, setPathInput, locate, locating, detailTab, setDetailTab, offset, provenance, rangeSummary, selectedValueLabel,
    previousRange: () => setOffset((value) => Math.max(0, value - REGISTRY_BROWSER_LAYOUT.valueRangeBytes)),
    nextRange: () => setOffset((value) => value + REGISTRY_BROWSER_LAYOUT.valueRangeBytes),
    rows: registryValueRows(rawValues, t),
    loadedCount: rawValues.length, declaredCount: pages[0]?.coverage.declaredCount ?? selectedKey?.valueCount ?? 0,
    partial: pages.some((page) => page.coverage.status === 'partial'),
    selectValue: (valueId: string) => {
      if (session && selectedKey) setValueSelection({ sessionId: session.sessionId, keyId: selectedKey.keyId, valueId });
      setOffset(0); setDetailTab('decoded'); setCopyStatus(undefined);
    },
    opening: hive.opening, requestedFileId: hive.requestedFileId,
    hiveError: errorText(hive.error), hivesError: errorText(hives.error), pathError: errorText(pathError),
    valuesError: errorText(values.error), rangeError: errorText(range.error),
    retryHive: async () => {
      if (!hive.requestedFileId) return;
      const current = await hives.refetch();
      if (current.data) await openHive(hive.requestedFileId, current.data.pages[0].sourceRevision);
    },
    loadChildren: (node: RegistryTreeNode) => { if (node.key) void tree.load(node.key, node.nextCursor); },
    copyPath: () => void copy([session?.hive.name, selectedKey?.path].filter(Boolean).join('\\')),
    copyValue: () => void copy(range.data?.decodedText ?? ''), copyStatus,
    keyTime,
    view,
    recovery: session?.recovery ?? recoveryFailureDetails(hive.error),
    switchView: (next: RegistryHiveView) => {
      if (next === view) return;
      const fileId = session?.hive.fileId ?? hive.requestedFileId;
      pendingViewPath.current = fileId ? { view: next, fileId, path: selectedKey?.path ?? '' } : undefined;
      setView(next);
      if (fileId) void openHive(fileId, undefined, next);
    },
  };
}

export type RegistryBrowserModel = ReturnType<typeof useRegistryBrowserModel>;
