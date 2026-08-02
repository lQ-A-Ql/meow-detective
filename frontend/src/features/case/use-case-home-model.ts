import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useCaseMetrics,
  useCreateCase,
  useCurrentCase,
  useDataSources,
  useDeleteCase,
  useDeleteDataSource,
  useOpenCase,
  useRecentCases,
  useRecentObjects,
  useRemoveCaseFromList,
  useRenameDataSource,
} from '@/features/case/hooks';
import { useImportDataSource } from '@/features/files/hooks';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
import { useAppSettings } from '@/features/settings/hooks';
import { useImportDataSourceDialogModel } from '@/features/import/use-import-data-source-dialog-model';
import { readLocalSettings } from '@/lib/settings';
import type { ImportDataSourceRequest } from '@/types/models';

function mutationError(error: unknown): string | null {
  return error instanceof Error ? error.message : null;
}

/** Owns Case Home queries, mutations, persisted UI state, and operation callbacks. */
export function useCaseHomeModel() {
  const { t } = useTranslation();
  const currentCaseQuery = useCurrentCase();
  const caseMetricsQuery = useCaseMetrics();
  const dataSourcesQuery = useDataSources();
  const recentCasesQuery = useRecentCases();
  const recentObjectsQuery = useRecentObjects();
  const jobsQuery = useJobsSnapshot();
  const warningsQuery = useWarnings();
  const appSettingsQuery = useAppSettings();
  const importMutation = useImportDataSource();
  const createCaseMutation = useCreateCase();
  const openCaseMutation = useOpenCase();
  const renameDataSourceMutation = useRenameDataSource();
  const deleteCaseMutation = useDeleteCase();
  const deleteDataSourceMutation = useDeleteDataSource();
  const removeCaseFromListMutation = useRemoveCaseFromList();
  const importDialog = useImportDataSourceDialogModel();
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [caseRoot, setCaseRoot] = useState(() => readLocalSettings().caseRoot);
  const [caseName, setCaseName] = useState('');
  const [openCasePath, setOpenCasePath] = useState('C:\\Cases\\case-001');
  const [editingDataSourceId, setEditingDataSourceId] = useState<string | undefined>();
  const [editingDataSourceName, setEditingDataSourceName] = useState('');
  const hasEditedCaseRoot = useRef(false);
  const currentCase = currentCaseQuery.data;
  const jobs = jobsQuery.data;
  const runningJobs = useMemo(() => jobs?.filter((job) => job.status === 'running') ?? [], [jobs]);
  const completedJobs = useMemo(() => jobs?.filter((job) => job.status === 'completed') ?? [], [jobs]);
  const partialJobCount = useMemo(() => jobs?.filter((job) => job.partial).length ?? 0, [jobs]);
  const recentCases = useMemo(() => recentCasesQuery.data ?? [], [recentCasesQuery.data]);

  useEffect(() => {
    if (appSettingsQuery.data && !hasEditedCaseRoot.current) {
      setCaseRoot(appSettingsQuery.data.caseRoot);
    }
  }, [appSettingsQuery.data]);

  const updateCaseRoot = useCallback((value: string) => {
    hasEditedCaseRoot.current = true;
    setCaseRoot(value);
  }, []);
  const createCase = useCallback(() => {
    createCaseMutation.mutate({ caseRoot, name: caseName });
  }, [caseName, caseRoot, createCaseMutation]);
  const openCase = useCallback((path: string) => {
    openCaseMutation.mutate(path);
  }, [openCaseMutation]);
  const deleteCase = useCallback((casePath: string) => {
    deleteCaseMutation.mutate(casePath, {
      onSuccess: () => {
        removeCaseFromListMutation.mutate(casePath);
      },
    });
  }, [deleteCaseMutation, removeCaseFromListMutation]);
  const renameDataSource = useCallback((dataSourceId: string, name: string) => {
    renameDataSourceMutation.mutate({ dataSourceId, name }, {
      onSuccess: () => {
        setEditingDataSourceId(undefined);
        setEditingDataSourceName('');
      },
    });
  }, [renameDataSourceMutation]);
  const importDataSource = useCallback((request: ImportDataSourceRequest) => {
    importMutation.mutate(request, {
      onSuccess: () => {
        setImportDialogOpen(false);
      },
    });
  }, [importMutation]);

  return {
    caseName,
    caseRoot,
    completedJobs,
    createCase,
    createCaseError: mutationError(createCaseMutation.error),
    createCasePending: createCaseMutation.isPending,
    currentCase,
    dataSources: dataSourcesQuery.data,
    deleteCase,
    deleteDataSource: deleteDataSourceMutation.mutate,
    editingDataSourceId,
    editingDataSourceName,
    importDataSource,
    importDialogOpen,
    importPending: importMutation.isPending,
    pickImportDirectoryPath: importDialog.pickDirectoryPath,
    pickImportSourcePath: importDialog.pickSourcePath,
    importButtonLabel: t('importDataSource.openButton'),
    metrics: caseMetricsQuery.data,
    openCase,
    openCaseError: mutationError(openCaseMutation.error),
    openCasePath,
    openCasePending: openCaseMutation.isPending,
    partialJobCount,
    recentCases,
    recentObjects: recentObjectsQuery.data,
    renameDataSource,
    runningJob: runningJobs[0],
    setCaseName,
    setEditingDataSourceId,
    setEditingDataSourceName,
    setImportDialogOpen,
    setOpenCasePath,
    updateCaseRoot,
    warnings: warningsQuery.data,
  };
}

export type CaseHomeModel = ReturnType<typeof useCaseHomeModel>;
