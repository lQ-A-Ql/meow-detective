import { useTranslation } from 'react-i18next';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useImportEventState } from '@/features/jobs/import-event-state';
import { useJobsSnapshot, useTraceItems, useWarnings } from '@/features/jobs/hooks';
import { buildBottomDrawerModel } from '@/features/jobs/model/bottom-drawer-model';
import { useUiStore } from '@/stores/ui-store';

export function useBottomDrawerModel() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id;
  const jobsQuery = useJobsSnapshot(caseId);
  const warningsQuery = useWarnings(caseId);
  const dataSourcesQuery = useDataSources();
  const traceQuery = useTraceItems(caseId);
  const importSignals = useImportEventState();
  const drawerOpen = useUiStore((state) => state.drawerOpen);
  const toggleDrawer = useUiStore((state) => state.toggleDrawer);

  return buildBottomDrawerModel({
    drawerOpen,
    toggleDrawer,
    jobs: jobsQuery.data,
    jobsError: jobsQuery.error,
    warnings: warningsQuery.data,
    warningsError: warningsQuery.error,
    dataSources: dataSourcesQuery.data,
    dataSourcesError: dataSourcesQuery.error,
    trace: traceQuery.data,
    traceError: traceQuery.error,
    importSignals,
    t,
  });
}

export type BottomDrawerModel = ReturnType<typeof useBottomDrawerModel>;
