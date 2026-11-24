import { TaskActivityMetrics } from '@/features/jobs/components/TaskActivityMetrics';
import { TaskProgressDetails } from '@/features/jobs/components/TaskProgressDetails';
import { useTaskProgressModel } from '@/features/jobs/hooks/use-task-progress-model';
import type { TaskProgress } from '@/types/models';

export function TaskProgressContainer({ task, compact = false }: { task: TaskProgress; compact?: boolean }) {
  const model = useTaskProgressModel(task);
  return compact ? <TaskActivityMetrics model={model} /> : <TaskProgressDetails model={model} />;
}
