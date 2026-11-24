import type { TaskProgressModel } from '@/features/jobs/hooks/use-task-progress-model';

export function TaskActivityMetrics({ model }: { model: TaskProgressModel }) {
  return (
    <span className="block truncate text-[10px] text-forensics-muted-light">
      {model.heartbeat ?? [model.work, model.count, model.rate, model.eta].filter(Boolean).join(' · ')}
    </span>
  );
}
