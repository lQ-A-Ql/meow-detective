import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import i18n from '@/i18n';
import type { EvtxEventSummary, EvtxEventView, EvtxSecurityEvent } from '@/types/models';
import { EventLogTabKey } from './use-event-log-panel-model-shared';

export function useEventLogPanelModel({
  summary,
  activeView,
  onActiveViewChange,
  hasMore = false,
  loadingMore = false,
  loadMoreFailed = false,
  onLoadMore,
  onRetryLoadMore,
  loadContextKey,
  loadStateKey,
}: {
  summary?: EvtxEventSummary;
  activeView?: EvtxEventView;
  onActiveViewChange?: (view: EvtxEventView) => void;
  hasMore?: boolean;
  loadingMore?: boolean;
  loadMoreFailed?: boolean;
  onLoadMore?: () => void;
  onRetryLoadMore?: () => unknown;
  loadContextKey?: string;
  loadStateKey?: string | number;
}) {

  const { t } = useTranslation();

  const [localActiveTab, setLocalActiveTab] = useState<EventLogTabKey>('boot');

  const activeTab = activeView ?? localActiveTab;

  const setActiveTab = (tab: EventLogTabKey) => {
    setLocalActiveTab(tab);
    onActiveViewChange?.(tab);
  };


  const info = summary ?? {
    status: 'unavailable' as const,
    pageTotal: 0,
    bootShutdownCount: 0,
    logonLogoffCount: 0,
    privilegeEscalationCount: 0,
    processExecutionCount: 0,
    accountManagementCount: 0,
    scheduledTaskCount: 0,
    applicationCrashCount: 0,
    softwareInstallationCount: 0,
    otherCount: 0,
    totalCount: 0,
    bootEvents: [],
    securityEvents: [],
    applicationEvents: [],
    warnings: [i18n.t('analysisPanels.events.summaryUnavailable')],
    generatedAt: '',
  };

  const tableLoadContextKey = JSON.stringify([
    loadContextKey ?? null,
    activeTab,
    info.generatedAt,
  ]);


  const logonEvents = useMemo(
    () => info.securityEvents.filter((e: EvtxSecurityEvent) =>
      ['logonSuccess', 'logonFailure', 'explicitCredentials'].includes(e.kind),
    ),
    [info.securityEvents],
  );

  const processEvents = useMemo(
    () => info.securityEvents.filter((e: EvtxSecurityEvent) => e.kind === 'processCreated'),
    [info.securityEvents],
  );

  const accountEvents = useMemo(
    () => info.securityEvents.filter((e: EvtxSecurityEvent) =>
      ['scheduledTaskCreated', 'scheduledTaskModified', 'accountCreated', 'groupMemberAdded'].includes(e.kind),
    ),
    [info.securityEvents],
  );
return { hasMore, loadingMore, loadMoreFailed, onLoadMore, onRetryLoadMore, loadContextKey, loadStateKey, t, activeTab, setActiveTab, info, tableLoadContextKey, logonEvents, processEvents, accountEvents };
}
