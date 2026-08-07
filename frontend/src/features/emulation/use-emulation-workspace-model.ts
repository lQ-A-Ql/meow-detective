import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { confirmEmulationBoot } from '@/features/emulation/boot-consent';
import { EMULATION_SESSIONS_QUERY_KEY } from '@/features/emulation/query-keys';
import {
  applyEmulationBypass,
  cleanupEmulationOsdata,
  getEmulationBypassAccounts,
  getEmulationPreflight,
  launchEmulation,
  listEmulationSessions,
  prepareEmulation,
  releaseEmulation,
} from '@/lib/api/emulation';
import { errorMessage } from '@/lib/errors';
import { openDialog as openPlatformDialog, singleDialogPath } from '@/lib/platform/dialog';
import type { DataSourceSummary, EmulationBypassAccount, EmulationBypassAction, EmulationOptions, EmulationPreflight, EmulationSessionStatus, EmulationState } from '@/types/models';

const ACTIVE_STATES = new Set<EmulationState>([
  'descriptorReady',
  'running',
  'quiescing',
]);

export interface EmulationSourceView {
  id: string;
  name: string;
  kind: string;
  platform: string;
  partitionCount: number;
  evidenceSize?: number;
}

export interface EmulationSessionView extends EmulationSessionStatus {
  sourceName: string;
  active: boolean;
  releasable: boolean;
}

export interface EmulationWorkspaceModel {
  caseLoaded: boolean;
  hasCase: boolean;
  caseName?: string;
  loading: boolean;
  sourceOptions: EmulationSourceView[];
  selectedSourceId: string;
  selectedSource?: EmulationSourceView;
  selectSource: (sourceId: string) => void;
  preflight?: EmulationPreflight;
  preflightLoading: boolean;
  recoveryIsoPath: string;
  bootRoute: 'recoveryMedia' | 'directSystem';
  pickRecoveryIso: () => Promise<void>;
  clearRecoveryIso: () => void;
  options: EmulationOptions;
  toggleOption: (key: keyof EmulationOptions) => void;
  osdataCleanupPartition?: number;
  cleanupOsdata: boolean;
  toggleCleanupOsdata: () => void;
  bypassPartition?: number;
  selectBypassPartition: (partition?: number) => void;
  bypassAccounts: EmulationBypassAccount[];
  bypassAccountsLoading: boolean;
  bypassRid?: number;
  selectBypassRid: (rid?: number) => void;
  bypassAction: EmulationBypassAction;
  selectBypassAction: (action: EmulationBypassAction) => void;
  sessions: EmulationSessionView[];
  metrics: {
    sourceCount: number;
    activeCount: number;
    runningCount: number;
    failedCount: number;
  };
  canStart: boolean;
  starting: boolean;
  releasingSessionId?: string;
  refreshing: boolean;
  error?: string;
  start: () => Promise<void>;
  release: (sessionId: string) => Promise<void>;
  refresh: () => Promise<void>;
}

function isActiveSession(session: EmulationSessionStatus): boolean {
  return ACTIVE_STATES.has(session.state);
}

function isEmulationSource(source: DataSourceSummary): boolean {
  return (source.kind === 'e01' || source.kind === 'raw')
    && (source.importState === 'ready' || source.importState === 'ready_metadata')
    && Boolean(source.sourceHash);
}

function toSourceView(source: DataSourceSummary): EmulationSourceView {
  return {
    id: source.id,
    name: source.name,
    kind: source.kind.toUpperCase(),
    platform: source.platform.toUpperCase(),
    partitionCount: source.partitions?.length ?? 0,
    evidenceSize: source.evidenceSize,
  };
}

export function useEmulationWorkspaceModel(): EmulationWorkspaceModel {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const currentCase = useCurrentCase();
  const dataSourcesQuery = useDataSources();
  const [selectedSourceId, setSelectedSourceId] = useState('');
  const [recoveryIsoPath, setRecoveryIsoPath] = useState('');
  const [options, setOptions] = useState<EmulationOptions>({
    network: false,
    clipboard: false,
    timeSync: false,
  });
  const [bypassPartition, setBypassPartition] = useState<number | undefined>(undefined);
  const [bypassRid, setBypassRid] = useState<number | undefined>(undefined);
  const [bypassAction, setBypassAction] = useState<EmulationBypassAction>('clearPassword');
  const [cleanupOsdata, setCleanupOsdata] = useState(true);
  const sessionsQuery = useQuery({
    queryKey: EMULATION_SESSIONS_QUERY_KEY,
    queryFn: listEmulationSessions,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    refetchInterval: (query) => query.state.data?.some(isActiveSession) ? 1500 : false,
    retry: false,
  });
  const sourceOptions = useMemo(
    () => (dataSourcesQuery.data ?? []).filter(isEmulationSource).map(toSourceView),
    [dataSourcesQuery.data],
  );

  useEffect(() => {
    if (!sourceOptions.some((source) => source.id === selectedSourceId)) {
      setSelectedSourceId(sourceOptions[0]?.id ?? '');
    }
  }, [selectedSourceId, sourceOptions]);

  const preflightQuery = useQuery({
    queryKey: ['emulation', 'preflight', selectedSourceId],
    queryFn: () => getEmulationPreflight(selectedSourceId),
    enabled: Boolean(selectedSourceId),
    retry: false,
  });
  const bypassAccountsQuery = useQuery({
    queryKey: ['emulation', 'bypass-accounts', selectedSourceId, bypassPartition],
    queryFn: () => getEmulationBypassAccounts(selectedSourceId, bypassPartition!),
    enabled: Boolean(selectedSourceId) && bypassPartition !== undefined,
    retry: false,
  });

  useEffect(() => {
    setBypassPartition(undefined);
    setBypassRid(undefined);
  }, [selectedSourceId]);

  useEffect(() => {
    setBypassRid(undefined);
  }, [bypassPartition]);

  const invalidateSessions = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: EMULATION_SESSIONS_QUERY_KEY });
  }, [queryClient]);
  const osdataCleanupPartition = useMemo(
    () => preflightQuery.data?.installs.find((install) => install.osdataPresent)?.partitionIndex,
    [preflightQuery.data],
  );
  const startMutation = useMutation({
    mutationFn: async (allowDirectBoot: boolean) => {
      if (!selectedSourceId) {
        throw new Error(t('emulationPage.errors.sourceRequired'));
      }
      const prepared = await prepareEmulation({
        dataSourceId: selectedSourceId,
        recoveryIsoPath: recoveryIsoPath || undefined,
        allowDirectBoot,
        options,
      });
      if (cleanupOsdata && osdataCleanupPartition !== undefined) {
        const cleanup = await cleanupEmulationOsdata({
          sessionId: prepared.sessionId,
          partitionIndex: osdataCleanupPartition,
        });
        if (cleanup.state === 'refusedNonEmpty') {
          throw new Error(t('emulationPage.errors.osdataNonEmpty'));
        }
      }
      if (bypassRid !== undefined && bypassPartition !== undefined) {
        await applyEmulationBypass({
          sessionId: prepared.sessionId,
          partitionIndex: bypassPartition,
          rid: bypassRid,
          action: bypassAction,
        });
      }
      return launchEmulation(prepared.sessionId);
    },
    onSuccess: invalidateSessions,
  });
  const releaseMutation = useMutation({
    mutationFn: (sessionId: string) => releaseEmulation(sessionId),
    onSuccess: invalidateSessions,
  });
  const sessions = useMemo<EmulationSessionView[]>(() => {
    const sourceNames = new Map(sourceOptions.map((source) => [source.id, source.name]));
    return (sessionsQuery.data ?? [])
      .map((session) => ({
        ...session,
        sourceName: sourceNames.get(session.dataSourceId) ?? session.dataSourceId,
        active: isActiveSession(session),
        releasable: session.state !== 'released',
      }))
      .sort((left, right) => Number(right.active) - Number(left.active)
        || left.sourceName.localeCompare(right.sourceName)
        || left.sessionId.localeCompare(right.sessionId));
  }, [sessionsQuery.data, sourceOptions]);
  const activeSourceIds = useMemo(
    () => new Set(sessions.filter((session) => session.active).map((session) => session.dataSourceId)),
    [sessions],
  );
  const selectedSource = sourceOptions.find((source) => source.id === selectedSourceId);
  const combinedError = currentCase.error
    ?? dataSourcesQuery.error
    ?? sessionsQuery.error
    ?? startMutation.error
    ?? releaseMutation.error;

  const pickRecoveryIso = useCallback(async () => {
    const selected = await openPlatformDialog({
      directory: false,
      multiple: false,
      filters: [{ name: 'WinPE ISO', extensions: ['iso'] }],
    });
    const path = singleDialogPath(selected);
    if (path) setRecoveryIsoPath(path);
  }, []);
  const clearRecoveryIso = useCallback(() => setRecoveryIsoPath(''), []);
  const toggleOption = useCallback((key: keyof EmulationOptions) => {
    setOptions((current) => ({ ...current, [key]: !current[key] }));
  }, []);
  const toggleCleanupOsdata = useCallback(() => {
    setCleanupOsdata((current) => !current);
  }, []);
  const start = useCallback(async () => {
    const allowDirectBoot = recoveryIsoPath.length === 0;
    if (!confirmEmulationBoot(recoveryIsoPath, t('fileBrowser.mount.directBootConfirm'))) return;
    await startMutation.mutateAsync(allowDirectBoot);
  }, [recoveryIsoPath, startMutation, t]);
  const release = useCallback(async (sessionId: string) => {
    await releaseMutation.mutateAsync(sessionId);
  }, [releaseMutation]);
  const refresh = useCallback(async () => {
    await Promise.all([dataSourcesQuery.refetch(), sessionsQuery.refetch()]);
  }, [dataSourcesQuery, sessionsQuery]);

  return {
    caseLoaded: currentCase.isSuccess,
    hasCase: Boolean(currentCase.data),
    caseName: currentCase.data?.name,
    loading: currentCase.isLoading || dataSourcesQuery.isLoading || sessionsQuery.isLoading,
    sourceOptions,
    selectedSourceId,
    selectedSource,
    selectSource: setSelectedSourceId,
    preflight: preflightQuery.data,
    preflightLoading: preflightQuery.isFetching,
    recoveryIsoPath,
    bootRoute: recoveryIsoPath ? 'recoveryMedia' : 'directSystem',
    pickRecoveryIso,
    clearRecoveryIso,
    options,
    toggleOption,
    osdataCleanupPartition,
    cleanupOsdata,
    toggleCleanupOsdata,
    bypassPartition,
    selectBypassPartition: setBypassPartition,
    bypassAccounts: bypassAccountsQuery.data ?? [],
    bypassAccountsLoading: bypassAccountsQuery.isFetching,
    bypassRid,
    selectBypassRid: setBypassRid,
    bypassAction,
    selectBypassAction: setBypassAction,
    sessions,
    metrics: {
      sourceCount: sourceOptions.length,
      activeCount: sessions.filter((session) => session.active).length,
      runningCount: sessions.filter((session) => session.state === 'running').length,
      failedCount: sessions.filter((session) => session.state === 'failedCleanupPending').length,
    },
    canStart: Boolean(selectedSourceId)
      && !activeSourceIds.has(selectedSourceId)
      && !startMutation.isPending
      && !releaseMutation.isPending,
    starting: startMutation.isPending,
    releasingSessionId: releaseMutation.isPending ? releaseMutation.variables : undefined,
    refreshing: dataSourcesQuery.isFetching || sessionsQuery.isFetching,
    error: combinedError ? errorMessage(combinedError, t('emulationPage.errors.operationFailed')) : undefined,
    start,
    release,
    refresh,
  };
}
