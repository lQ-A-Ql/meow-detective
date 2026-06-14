import { useState, useMemo } from 'react';
import { ChevronDown, ChevronRight, Clock, CheckCircle2, XCircle, AlertTriangle, Pause } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/app/components/ui/card';
import { Badge } from '@/app/components/ui/badge';
import { Progress } from '@/app/components/ui/progress';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { DenseDataTable, type DenseColumn } from '@/components/tables/DenseDataTable';
import type { BatchJob, BatchJobStatus } from '@/types/models';

interface BatchHistoryProps {
  jobs: BatchJob[];
  onSelectJob?: (job: BatchJob) => void;
}

const STATUS_CONFIG: Record<BatchJobStatus, { badge: 'default' | 'secondary' | 'destructive' | 'outline'; label: string; icon: React.ReactNode }> = {
  pending: { badge: 'secondary' as const, label: 'Pending', icon: <Clock size={12} /> },
  running: { badge: 'default' as const, label: 'Running', icon: <Clock size={12} /> },
  paused: { badge: 'secondary' as const, label: 'Paused', icon: <Pause size={12} /> },
  completed: { badge: 'outline' as const, label: 'Completed', icon: <CheckCircle2 size={12} className="text-green-600" /> },
  failed: { badge: 'destructive' as const, label: 'Failed', icon: <XCircle size={12} className="text-destructive" /> },
  cancelled: { badge: 'secondary' as const, label: 'Cancelled', icon: <AlertTriangle size={12} /> },
};

function formatDurationMs(ms?: number): string {
  if (ms == null) return '--';
  const seconds = Math.floor(ms / 1000);
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export function BatchHistory({ jobs, onSelectJob }: BatchHistoryProps) {
  const [expandedJobId, setExpandedJobId] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<string>('createdAt');
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('desc');

  const sortedJobs = useMemo(() => {
    const sorted = [...jobs];
    sorted.sort((a, b) => {
      let comparison = 0;
      switch (sortKey) {
        case 'name':
          comparison = a.name.localeCompare(b.name);
          break;
        case 'status':
          comparison = a.status.localeCompare(b.status);
          break;
        case 'duration':
          comparison = (a.elapsedMs ?? 0) - (b.elapsedMs ?? 0);
          break;
        case 'fileCount':
          comparison = a.fileCount - b.fileCount;
          break;
        case 'artifactCount':
          comparison = a.artifactCount - b.artifactCount;
          break;
        case 'createdAt':
        default:
          comparison = new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime();
          break;
      }
      return sortDirection === 'asc' ? comparison : -comparison;
    });
    return sorted;
  }, [jobs, sortKey, sortDirection]);

  const columns: DenseColumn<BatchJob>[] = [
    {
      key: 'name',
      title: 'Name',
      sortable: true,
      sortKey: 'name',
      className: 'w-[240px]',
      render: (row) => (
        <div className="flex items-center gap-1.5">
          <span className="truncate font-medium">{row.name}</span>
        </div>
      ),
    },
    {
      key: 'status',
      title: 'Status',
      sortable: true,
      sortKey: 'status',
      className: 'w-[100px]',
      render: (row) => {
        const config = STATUS_CONFIG[row.status];
        return (
          <Badge variant={config.badge} className="gap-1 text-[10px]">
            {config.icon}
            {config.label}
          </Badge>
        );
      },
    },
    {
      key: 'duration',
      title: 'Duration',
      sortable: true,
      sortKey: 'duration',
      className: 'w-[90px]',
      render: (row) => (
        <span className="font-mono text-[11px]">{formatDurationMs(row.elapsedMs)}</span>
      ),
    },
    {
      key: 'fileCount',
      title: 'Files',
      sortable: true,
      sortKey: 'fileCount',
      className: 'w-[80px]',
      render: (row) => <span className="font-mono text-[11px]">{row.fileCount.toLocaleString()}</span>,
    },
    {
      key: 'artifactCount',
      title: 'Artifacts',
      sortable: true,
      sortKey: 'artifactCount',
      className: 'w-[80px]',
      render: (row) => <span className="font-mono text-[11px]">{row.artifactCount.toLocaleString()}</span>,
    },
    {
      key: 'createdAt',
      title: 'Created',
      sortable: true,
      sortKey: 'createdAt',
      className: 'w-[120px]',
      render: (row) => (
        <span className="font-mono text-[11px]">{new Date(row.createdAt).toLocaleDateString()}</span>
      ),
    },
    {
      key: 'expand',
      title: '',
      className: 'w-[40px]',
      render: (row) => (
        <span className="text-muted-foreground">
          {expandedJobId === row.id ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
      ),
    },
  ];

  const expandedJob = useMemo(
    () => jobs.find((j) => j.id === expandedJobId) ?? null,
    [jobs, expandedJobId],
  );

  const handleRowClick = (row: BatchJob) => {
    setExpandedJobId((prev) => (prev === row.id ? null : row.id));
    onSelectJob?.(row);
  };

  const handleSort = (key: string) => {
    if (sortKey === key) {
      setSortDirection((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDirection('asc');
    }
  };

  return (
    <Card className="w-full max-w-4xl">
      <CardHeader>
        <CardTitle className="text-[14px] font-semibold">Batch Job History</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3 p-0">
        <DenseDataTable
          columns={columns}
          rows={sortedJobs}
          getRowKey={(row) => row.id}
          selectedRowKey={expandedJobId ?? undefined}
          onRowClick={handleRowClick}
          emptyTitle="No batch jobs"
          emptyDescription="Create a new batch job to get started."
          sortKey={sortKey}
          sortDirection={sortDirection}
          onSort={handleSort}
        />

        {/* Expanded phase details */}
        {expandedJob && (
          <div className="border-t px-6 pb-4 pt-3">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-[12px] font-medium">{expandedJob.name} - Phases</span>
                <span className="font-mono text-[11px] text-muted-foreground">
                  {expandedJob.plan.dataSourceCount} source{expandedJob.plan.dataSourceCount !== 1 ? 's' : ''}
                  {' · '}
                  {expandedJob.plan.resourceLimits.memoryMb} MB / {expandedJob.plan.resourceLimits.threadCount} threads
                </span>
              </div>
              <ScrollArea className="max-h-48">
                <div className="space-y-1.5">
                  {expandedJob.phases.map((phase) => {
                    const config = STATUS_CONFIG[phase.state as BatchJobStatus] ?? STATUS_CONFIG.pending;
                    return (
                      <div
                        key={phase.phase}
                        className="flex items-center gap-3 rounded border px-3 py-2"
                      >
                        <div className="shrink-0">{config.icon}</div>
                        <div className="min-w-[100px] text-[12px] font-medium">{phase.phase}</div>
                        <div className="flex-1 space-y-1">
                          <Progress value={phase.progress} className="h-1" />
                          {phase.detail && (
                            <p className="truncate text-[11px] text-muted-foreground">{phase.detail}</p>
                          )}
                        </div>
                        <Badge variant={config.badge} className="shrink-0 text-[10px]">
                          {config.label}
                        </Badge>
                      </div>
                    );
                  })}
                </div>
              </ScrollArea>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
