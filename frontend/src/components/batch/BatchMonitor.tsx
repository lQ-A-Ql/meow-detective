import { useRef, useEffect, useMemo } from 'react';
import { Pause, Play, X, Clock, AlertTriangle, CheckCircle2, XCircle, Loader2 } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from '@/app/components/ui/card';
import { Badge } from '@/app/components/ui/badge';
import { Progress } from '@/app/components/ui/progress';
import { ScrollArea } from '@/app/components/ui/scroll-area';
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

function formatDuration(ms?: number): string {
  if (ms == null) return '--';
  const seconds = Math.floor(ms / 1000);
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function formatEta(ms?: number): string {
  if (ms == null) return '--';
  return `ETA: ${formatDuration(ms)}`;
}

export function BatchMonitor({ job, onPause, onResume, onCancel, isPausing, isResuming, isCancelling }: BatchMonitorProps) {
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [job.logTail]);

  const isRunning = job.status === 'running';
  const isPaused = job.status === 'paused';
  const isTerminal = job.status === 'completed' || job.status === 'failed' || job.status === 'cancelled';

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

  const sortedLogLines = useMemo(
    () => [...job.logTail].reverse(),
    [job.logTail],
  );

  return (
    <Card className="w-full max-w-2xl">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <CardTitle className="truncate text-[14px] font-semibold">{job.name}</CardTitle>
              {statusBadge}
            </div>
          </div>
          <div className="flex items-center gap-1 shrink-0">
            <Clock size={13} className="text-muted-foreground" />
            <span className="font-mono text-[11px] text-muted-foreground">
              {formatDuration(job.elapsedMs)}
            </span>
            {job.etaMs != null && !isTerminal && (
              <span className="font-mono text-[11px] text-muted-foreground">
                {formatEta(job.etaMs)}
              </span>
            )}
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Overall progress */}
        <div className="space-y-1">
          <div className="flex items-center justify-between">
            <span className="text-[11px] text-muted-foreground">Overall Progress</span>
            <span className="font-mono text-[11px]">{Math.round(job.progress)}%</span>
          </div>
          <Progress value={job.progress} className="h-2" />
        </div>

        {/* Phase list */}
        <div className="space-y-2">
          <span className="text-[11px] font-medium text-muted-foreground">Phases</span>
          {job.phases.map((phase) => {
            const config = PHASE_STATE_CONFIG[phase.state];
            return (
              <div key={phase.phase} className="space-y-1">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    {phase.state === 'completed' && <CheckCircle2 size={13} className="text-green-600" />}
                    {phase.state === 'failed' && <XCircle size={13} className="text-destructive" />}
                    {phase.state === 'running' && <Loader2 size={13} className="animate-spin text-primary" />}
                    {phase.state === 'pending' && <div className="size-[13px] rounded-full border border-muted-foreground/30" />}
                    {phase.state === 'skipped' && <AlertTriangle size={13} className="text-muted-foreground" />}
                    <span className="text-[12px]">{phase.phase}</span>
                  </div>
                  <Badge variant={config.badge} className="text-[10px]">
                    {config.label}
                  </Badge>
                </div>
                <Progress value={phase.progress} className="h-1" />
                {phase.detail && (
                  <p className="truncate pl-5 text-[11px] text-muted-foreground">{phase.detail}</p>
                )}
              </div>
            );
          })}
        </div>

        {/* Stats */}
        <div className="flex gap-4 rounded border p-2">
          <div className="text-center">
            <div className="font-mono text-[13px] font-medium">{job.fileCount.toLocaleString()}</div>
            <div className="text-[10px] text-muted-foreground">Files</div>
          </div>
          <div className="text-center">
            <div className="font-mono text-[13px] font-medium">{job.artifactCount.toLocaleString()}</div>
            <div className="text-[10px] text-muted-foreground">Artifacts</div>
          </div>
          <div className="text-center">
            <div className="font-mono text-[13px] font-medium">
              {job.plan.dataSourceCount}
            </div>
            <div className="text-[10px] text-muted-foreground">Sources</div>
          </div>
        </div>

        {/* Log tail */}
        {sortedLogLines.length > 0 && (
          <div className="space-y-1">
            <span className="text-[11px] font-medium text-muted-foreground">Log</span>
            <ScrollArea className="h-28 rounded border bg-gray-950 p-2 font-mono text-[11px]">
              <div className="space-y-0.5">
                {sortedLogLines.map((line, i) => (
                  <div
                    key={i}
                    className={`flex gap-2 ${
                      line.level === 'error'
                        ? 'text-red-400'
                        : line.level === 'warn'
                          ? 'text-yellow-400'
                          : 'text-green-400/80'
                    }`}
                  >
                    <span className="shrink-0 text-gray-500">
                      {new Date(line.ts).toLocaleTimeString()}
                    </span>
                    <span className="break-all">{line.message}</span>
                  </div>
                ))}
                <div ref={logEndRef} />
              </div>
            </ScrollArea>
          </div>
        )}
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
