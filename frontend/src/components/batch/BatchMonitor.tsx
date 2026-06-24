import { useMemo } from 'react';
import { Pause, Play, X, AlertTriangle, CheckCircle2, XCircle, Loader2 } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from '@/app/components/ui/card';
import { Badge } from '@/app/components/ui/badge';
import { Progress } from '@/app/components/ui/progress';
import type { BatchJob, BatchPhaseState } from '@/types/models';

interface BatchMonitorProps {
  job: BatchJob;
  onPause?: () => void;
  onResume?: () => void;
  onCancel?: () => void;
  isPausing?: boolean;
  isResuming?: boolean;
  isCancelling?: boolean;
}

const PHASE_STATE_CONFIG: Record<BatchPhaseState, { badge: 'default' | 'secondary' | 'destructive' | 'outline'; label: string }> = {
  pending: { badge: 'secondary', label: 'Pending' },
  running: { badge: 'default', label: 'Running' },
  completed: { badge: 'outline', label: 'Completed' },
  failed: { badge: 'destructive', label: 'Failed' },
  skipped: { badge: 'secondary', label: 'Skipped' },
};

function deriveProgressFromPhases(job: BatchJob): number {
  if (job.phases.length === 0) return 0;
  const total = job.phases.reduce((sum, phase) => sum + phase.progress, 0);
  return total / job.phases.length;
}

export function BatchMonitor({ job, onPause, onResume, onCancel, isPausing, isResuming, isCancelling }: BatchMonitorProps) {
  const isRunning = job.status === 'running';
  const isPaused = job.status === 'paused';
  const isTerminal = job.status === 'completed' || job.status === 'failed' || job.status === 'cancelled';
  const progress = deriveProgressFromPhases(job);
  const dataSourceCount = job.plan.dataSourceRefs.length;

  const statusBadge = useMemo(() => {
    switch (job.status) {
      case 'running':
        return <Badge variant="default" className="text-[10px]">Running</Badge>;
      case 'paused':
        return <Badge variant="secondary" className="text-[10px]">Paused</Badge>;
      case 'completed':
        return <Badge variant="outline" className="text-[10px] border-green-300 text-green-700">Completed</Badge>;
      case 'failed':
        return <Badge variant="destructive" className="text-[10px]">Failed</Badge>;
      case 'cancelled':
        return <Badge variant="secondary" className="text-[10px]">Cancelled</Badge>;
      default:
        return <Badge variant="secondary" className="text-[10px]">Pending</Badge>;
    }
  }, [job.status]);

  return (
    <Card className="w-full max-w-2xl">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <CardTitle className="truncate text-[14px] font-semibold">{job.label}</CardTitle>
              {statusBadge}
            </div>
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Overall progress */}
        <div className="space-y-1">
          <div className="flex items-center justify-between">
            <span className="text-[11px] text-muted-foreground">Overall Progress</span>
            <span className="font-mono text-[11px]">{Math.round(progress)}%</span>
          </div>
          <Progress value={progress} className="h-2" />
        </div>

        {/* Phase list */}
        <div className="space-y-2">
          <span className="text-[11px] font-medium text-muted-foreground">Phases</span>
          {job.phases.map((phase) => {
            const config = PHASE_STATE_CONFIG[phase.state];
            return (
              <div key={phase.kind} className="space-y-1">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    {phase.state === 'completed' && <CheckCircle2 size={13} className="text-green-600" />}
                    {phase.state === 'failed' && <XCircle size={13} className="text-destructive" />}
                    {phase.state === 'running' && <Loader2 size={13} className="animate-spin text-primary" />}
                    {phase.state === 'pending' && <div className="size-[13px] rounded-full border border-muted-foreground/30" />}
                    {phase.state === 'skipped' && <AlertTriangle size={13} className="text-muted-foreground" />}
                    <span className="text-[12px]">{phase.kind}</span>
                  </div>
                  <Badge variant={config.badge} className="text-[10px]">
                    {config.label}
                  </Badge>
                </div>
                <Progress value={phase.progress} className="h-1" />
              </div>
            );
          })}
        </div>

        {/* Stats */}
        <div className="flex gap-4 rounded border p-2">
          <div className="text-center">
            <div className="font-mono text-[13px] font-medium">
              {dataSourceCount}
            </div>
            <div className="text-[10px] text-muted-foreground">Sources</div>
          </div>
        </div>
      </CardContent>

      {!isTerminal && (
        <CardFooter className="gap-2">
          {isRunning && onPause && (
            <Button variant="outline" size="sm" onClick={onPause} disabled={isPausing}>
              {isPausing ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Pause size={14} />
              )}
              Pause
            </Button>
          )}
          {isPaused && onResume && (
            <Button variant="outline" size="sm" onClick={onResume} disabled={isResuming}>
              {isResuming ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Play size={14} />
              )}
              Resume
            </Button>
          )}
          {onCancel && (
            <Button variant="destructive" size="sm" onClick={onCancel} disabled={isCancelling}>
              {isCancelling ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <X size={14} />
              )}
              Cancel
            </Button>
          )}
        </CardFooter>
      )}
    </Card>
  );
}
