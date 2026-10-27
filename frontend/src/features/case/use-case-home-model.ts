import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useCreateCase,
  useCurrentCase,
  useDataSources,
  useDeleteCase,
  useDeleteDataSource,
  useOpenCase,
  useRecentCases,
  useRemoveCaseFromList,
  useRenameDataSource,
} from '@/features/case/hooks';
import { useImportDataSource } from '@/features/files/hooks';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import type { DigestAlgorithm } from '@/types/digest';
import { useJobsSnapshot } from '@/features/jobs/hooks';
import { useAppSettings } from '@/features/settings/hooks';
import { useImportDataSourceDialogModel } from '@/features/import/use-import-data-source-dialog-model';
import { errorMessage } from '@/lib/errors';
import { readLocalSettings } from '@/lib/settings';
import type { ImportDataSourceRequest } from '@/types/models';
import type { DataSourceSummary, RecentCase } from '@/types/models';

function mutationError(error: unknown): string | null {
  return error == null ? null : errorMessage(error);
}

/** Owns Case Home queries, mutations, persisted UI state, and operation callbacks. */
export function useCaseHomeModel() {
  const { t } = useTranslation();
  const currentCaseQuery = useCurrentCase();
  const dataSourcesQuery = useDataSources();
  const recentCasesQuery = useRecentCases();
  const jobsQuery = useJobsSnapshot(currentCaseQuery.data?.id);
  const appSettingsQuery = useAppSettings();
  const importMutation = useImportDataSource();
  const logicalDiskDigestTask = useEvidenceTask(currentCaseQuery.data?.id, 'source-digest');
  const createCaseMutation = useCreateCase();
  const openCaseMutation = useOpenCase();
  const renameDataSourceMutation = useRenameDataSource();
  const deleteCaseMutation = useDeleteCase();
  const deleteDataSourceMutation = useDeleteDataSource();
  const removeCaseFromListMutation = useRemoveCaseFromList();
  const importDialog = useImportDataSourceDialogModel();
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [deleteCaseDialogOpen, setDeleteCaseDialogOpen] = useState(false);
  const [caseRoot, setCaseRoot] = useState(() => readLocalSettings().caseRoot);
  const [caseName, setCaseName] = useState('');
  const [openCasePath, setOpenCasePath] = useState('');
  const [editingDataSourceId, setEditingDataSourceId] = useState<string | undefined>();
  const [editingDataSourceName, setEditingDataSourceName] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<
    | { kind: 'case'; value: RecentCase }
    | { kind: 'dataSource'; value: DataSourceSummary }
    | undefined
  >();
  const hasEditedCaseRoot = useRef(false);
  const currentCase = currentCaseQuery.data;
  const jobs = jobsQuery.data;
  const runningJobs = useMemo(() => jobs?.filter((job) => job.status === 'running') ?? [], [jobs]);
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
  const requestDeleteCase = useCallback((recentCase: RecentCase) => {
    setDeleteCaseDialogOpen(false);
    setDeleteTarget({ kind: 'case', value: recentCase });
  }, []);
  const requestDeleteDataSource = useCallback((source: DataSourceSummary) => setDeleteTarget({ kind: 'dataSource', value: source }), []);
  const confirmDelete = useCallback(() => {
    if (!deleteTarget) return;
    if (deleteTarget.kind === 'case') {
      deleteCase(deleteTarget.value.caseRoot);
    } else {
      deleteDataSourceMutation.mutate(deleteTarget.value.id);
    }
    setDeleteTarget(undefined);
  }, [deleteCase, deleteDataSourceMutation, deleteTarget]);
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
  const calculateLogicalDiskDigest = useCallback((dataSourceId: string, algorithm: DigestAlgorithm) => {
    logicalDiskDigestTask.start({ kind: 'digest', request: { scope: 'logicalDisk', dataSourceId, algorithm } });
  }, [logicalDiskDigestTask]);

  return {
    caseName,
    caseRoot,
    logicalDiskDigest: logicalDiskDigestTask.task?.result?.kind === 'digest' ? logicalDiskDigestTask.task.result.result : undefined,
    logicalDiskDigestSourceId: logicalDiskDigestTask.request?.kind === 'digest' ? logicalDiskDigestTask.request.request.dataSourceId : undefined,
    logicalDiskDigestLoading: logicalDiskDigestTask.loading,
    logicalDiskDigestErrorText: logicalDiskDigestTask.errorText,
    logicalDiskDigestTask,
    calculateLogicalDiskDigest,
    createCase,
    createCaseError: mutationError(createCaseMutation.error),
    createCasePending: createCaseMutation.isPending,
    currentCase,
    dataSources: dataSourcesQuery.data,
    deleteCase,
    deleteCaseDialogOpen,
    setDeleteCaseDialogOpen,
    deleteTarget,
    deleteDataSource: deleteDataSourceMutation.mutate,
    confirmDelete,
    editingDataSourceId,
    editingDataSourceName,
    importDataSource,
    importDialogOpen,
    importPending: importMutation.isPending,
    pickImportDirectoryPath: importDialog.pickDirectoryPath,
    pickImportSourcePath: importDialog.pickSourcePath,
    listLocalDisks: importDialog.listLocalDisks,
    importButtonLabel: t('importDataSource.openButton'),
    openCase,
    openCaseError: mutationError(openCaseMutation.error),
    openCasePath,
    openCasePending: openCaseMutation.isPending,
    recentCases,
    requestDeleteCase,
    requestDeleteDataSource,
    renameDataSource,
    runningJob: runningJobs[0],
    setCaseName,
    setEditingDataSourceId,
    setEditingDataSourceName,
    setDeleteTarget,
    setImportDialogOpen,
    setOpenCasePath,
    updateCaseRoot,
  };
}

export type CaseHomeModel = ReturnType<typeof useCaseHomeModel>;
