import { useEffect, useMemo, useRef, useState } from 'react';
import { Upload } from 'lucide-react';
import {
  useCreateCase,
  useCurrentCase,
  useOpenCase,
  useCaseMetrics,
  useDataSources,
  useDeleteCase,
  useDeleteDataSource,
  useRecentCases,
  useRecentObjects,
  useRemoveCaseFromList,
  useRenameDataSource,
} from '@/features/case/hooks';
import { useImportDataSource } from '@/features/files/hooks';
import { useJobsSnapshot, useWarnings } from '@/features/jobs/hooks';
import { getAppSettings } from '@/lib/api/settings';
import { readLocalSettings } from '@/lib/settings';
import { CaseMetricsStrip, RecentTasksPanel, DataSourcesPanel, RecentObjectsPanel } from './CaseOverview';
import { CaseWelcomeForms } from './CaseActions';
import { ImportDataSourceDialog } from '@/components/import/ImportDataSourceDialog';

export function CaseHome() {
  const { data: currentCase } = useCurrentCase();
  const { data: metrics } = useCaseMetrics();
  const { data: dataSources } = useDataSources();
  const { data: recentCases } = useRecentCases();
  const { data: recentObjects } = useRecentObjects();
  const { data: jobs } = useJobsSnapshot();
  const { data: warnings } = useWarnings();
  const importMutation = useImportDataSource();
  const createCaseMutation = useCreateCase();
  const openCaseMutation = useOpenCase();
  const renameDataSourceMutation = useRenameDataSource();
  const deleteCaseMutation = useDeleteCase();
  const deleteDataSourceMutation = useDeleteDataSource();
  const removeCaseFromListMutation = useRemoveCaseFromList();

  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [caseRoot, setCaseRoot] = useState(() => readLocalSettings().caseRoot);
  const [caseName, setCaseName] = useState('');
  const [openCasePath, setOpenCasePath] = useState('C:\\Cases\\case-001');
  const [editingDataSourceId, setEditingDataSourceId] = useState<string | undefined>();
  const [editingDataSourceName, setEditingDataSourceName] = useState('');
  const hasEditedCaseRoot = useRef(false);

  const runningJobs = jobs?.filter((job) => job.status === 'running') ?? [];
  const runningJob = runningJobs[0];
  const completedJobs = jobs?.filter((job) => job.status === 'completed') ?? [];
  const partialJobCount = jobs?.filter((job) => job.partial).length ?? 0;
  const sortedRecentCases = useMemo(() => recentCases ?? [], [recentCases]);

  useEffect(() => {
    let cancelled = false;
    getAppSettings()
      .then((settings) => {
        if (!cancelled && !hasEditedCaseRoot.current) {
          setCaseRoot(settings.caseRoot);
        }
      })
      .catch(() => {
        // Keep local fallback if persisted settings are temporarily unavailable.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!currentCase) {
    return (
      <CaseWelcomeForms
        caseRoot={caseRoot}
        setCaseRoot={(value) => {
          hasEditedCaseRoot.current = true;
          setCaseRoot(value);
        }}
        caseName={caseName}
        setCaseName={setCaseName}
        onCreateCase={() => createCaseMutation.mutate({ caseRoot, name: caseName })}
        createPending={createCaseMutation.isPending}
        createError={(createCaseMutation.error as Error)?.message ?? null}
        openCasePath={openCasePath}
        setOpenCasePath={setOpenCasePath}
        onOpenCase={(path) => openCaseMutation.mutate(path)}
        openPending={openCaseMutation.isPending}
        openError={(openCaseMutation.error as Error)?.message ?? null}
        recentCases={sortedRecentCases}
        onDeleteCase={(root) => {
          deleteCaseMutation.mutate(root, {
            onSuccess: () => { removeCaseFromListMutation.mutate(root); },
          });
        }}
      />
    );
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-hidden">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="flex items-start justify-between gap-6">
          <div>
            <div className="font-serif text-2xl text-[#111] mb-1 tracking-tight">案件 #{currentCase.number ?? '-'}</div>
            <div className="text-[#666] font-mono text-[11px]">{currentCase.name}</div>
            <div className="mt-3 flex flex-wrap gap-2 text-[10px] uppercase tracking-wider text-[#666]">
              <span className="border border-[#d9d9d9] bg-white px-2 py-1">当前状态: 活跃</span>
              <span className="border border-[#d9d9d9] bg-white px-2 py-1">数据源: {metrics?.dataSourceCount ?? 0}</span>
              <span className="border border-[#e7d9b4] bg-white px-2 py-1">告警: {warnings?.length ?? 0}</span>
              <span className="border border-[#e7d9b4] bg-white px-2 py-1">部分完成任务: {partialJobCount}</span>
            </div>
          </div>
          <div className="flex gap-8 text-right">
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">状态</div>
              <div className="flex items-center gap-1.5 text-[#111] text-[13px] justify-end">
                <div className="w-1.5 h-1.5 rounded-full bg-[#111]"></div> 活跃
              </div>
            </div>
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">创建时间</div>
              <div className="text-[#111] text-[13px] font-mono">{currentCase.createdAt}</div>
            </div>
            <div>
              <div className="text-[#888] text-[10px] uppercase tracking-wider mb-1">检验人</div>
              <div className="text-[#111] text-[13px]">{currentCase.examiner ?? '-'}</div>
            </div>
            <button
              onClick={() => setImportDialogOpen(true)}
              className="flex items-center gap-1.5 border border-[#111] px-3 py-1.5 text-[12px] hover:bg-[#111] hover:text-white transition-colors"
            >
              <Upload size={12} /> 导入数据源
            </button>
          </div>
        </div>
      </div>

      <ImportDataSourceDialog
        open={importDialogOpen}
        onOpenChange={setImportDialogOpen}
        onImport={(sourcePath) => importMutation.mutate(sourcePath)}
        importPending={importMutation.isPending}
      />

      <CaseMetricsStrip
        dataSourceCount={metrics?.dataSourceCount ?? 0}
        indexedFileCount={metrics?.indexedFileCount ?? 0}
        timelineEventCount={metrics?.timelineEventCount ?? 0}
        artifactCount={metrics?.artifactCount ?? 0}
      />

      <div className="flex-1 flex min-h-0">
        <RecentTasksPanel
          runningJob={runningJob}
          completedJobs={completedJobs}
          partialJobCount={partialJobCount}
        />

        <div className="w-1/2 flex flex-col min-h-0 bg-[#fafafa]">
          <DataSourcesPanel
            dataSources={dataSources}
            editingDataSourceId={editingDataSourceId}
            editingDataSourceName={editingDataSourceName}
            setEditingDataSourceId={setEditingDataSourceId}
            setEditingDataSourceName={setEditingDataSourceName}
            onRename={(dataSourceId, name) => {
              renameDataSourceMutation.mutate(
                { dataSourceId, name },
                { onSuccess: () => { setEditingDataSourceId(undefined); setEditingDataSourceName(''); } },
              );
            }}
            onDelete={(id) => deleteDataSourceMutation.mutate(id)}
          />
          <RecentObjectsPanel recentObjects={recentObjects} />
        </div>
      </div>
    </div>
  );
}
