import { useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { RegistryHardwareClient } from '@/lib/api/registry-hardware';
import type { RegistryHiveView } from '@/types/registryBrowser';
import { useRegistryHives } from '@/features/registry-browser/hooks/use-registry-hives';
import { useRegistrySession } from '@/features/registry-browser/hooks/use-registry-session';
import { useRegistrySessionCache } from '@/features/registry-browser/hooks/use-registry-session-cache';
import { registryBrowserErrorCode } from '@/features/registry-browser/errors';
import type { RegistryBrowserContext } from '@/features/registry-browser/types';
import { hardwareGroups, hardwareMemorySummary } from './hardware-model';

export function useRegistryHardwareModel(client: RegistryHardwareClient, context: RegistryBrowserContext) {
  const { t } = useTranslation();
  const hives = useRegistryHives(client, context);
  const systems = useMemo(() => (hives.data?.hives ?? []).filter((hive) => hive.name.toUpperCase() === 'SYSTEM'), [hives.data]);
  const [selectedId, setSelectedId] = useState<string>();
  const [view, setView] = useState<RegistryHiveView>('base');
  const [attempt, setAttempt] = useState(0);
  const retryEpoch = useRef(0);
  useEffect(() => () => { retryEpoch.current += 1; }, []);
  const selected = systems.find((hive) => hive.fileId === selectedId) ?? systems[0];
  const revision = hives.data?.sourceRevision ?? context.sourceRevision;
  const hive = useRegistrySession(client, { ...context, sourceRevision: revision });
  const session = hive.session;
  useRegistrySessionCache(session?.sessionId);
  useEffect(() => {
    if (!systems.length && hives.hasNextPage && !hives.isFetchingNextPage && !hives.isFetchNextPageError && !hives.isError) void hives.fetchNextPage();
  }, [systems.length, hives.hasNextPage, hives.isFetchingNextPage, hives.isFetchNextPageError, hives.isError, hives.fetchNextPage]);
  useEffect(() => { if (selected?.fileId) void hive.openHive(selected.fileId, revision, view); }, [selected?.fileId, revision, view, attempt, hive.openHive]);
  const sourceMatches = session?.source.fileId === selected?.fileId && session?.source.view === view;
  const summary = useQuery({
    queryKey: ['registry-browser', 'hardware', context.caseId, context.dataSourceId, revision, session?.sessionId],
    enabled: Boolean(session) && sourceMatches && !hive.opening,
    queryFn: async () => {
      const result = await client.readHardware({ sessionId: session!.sessionId });
      const expected = session!.source;
      if (result.source.caseId !== expected.caseId || result.source.dataSourceId !== expected.dataSourceId
        || result.source.fileId !== expected.fileId || result.source.sourceRevision !== expected.sourceRevision || result.source.view !== expected.view) {
        throw Object.assign(new Error('sourceMismatch'), { code: 'expiredSession' });
      }
      return result;
    },
    retry: false, gcTime: 0, refetchOnWindowFocus: false,
  });
  const devices = sourceMatches && !hive.opening ? summary.data?.devices ?? [] : [];
  const error = hives.error ?? hive.error ?? summary.error;
  const errorText = error ? t(`registryBrowser.errors.${registryBrowserErrorCode(error)}`) : undefined;
  return {
    t, systems, selected, selectHive: setSelectedId, view, setView, session,
    loading: hives.isPending || hive.opening || (Boolean(session) && summary.isPending) || (!systems.length && hives.isFetchingNextPage)
      || (Boolean(selected) && !sourceMatches && !hive.error),
    errorText, hasMore: hives.hasNextPage, loadingMore: hives.isFetchingNextPage,
    loadMore: () => void hives.fetchNextPage(),
    retry: () => {
      const token = ++retryEpoch.current;
      void hives.refetch().then((result) => {
        if (token === retryEpoch.current && result.isSuccess && result.data.pages[0]?.sourceRevision === revision) setAttempt((value) => value + 1);
      });
    },
    groups: hardwareGroups(devices, t), memory: hardwareMemorySummary(devices, t),
    warnings: sourceMatches ? summary.data?.warnings ?? [] : [], recovery: sourceMatches ? session?.recovery : undefined,
  };
}

export type RegistryHardwareModel = ReturnType<typeof useRegistryHardwareModel>;
