import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { formatBytes } from '@/lib/format-bytes';
import {
  taskDuration,
  taskHeartbeatState,
  taskStageLabel,
  taskStageProgress,
} from '@/features/jobs/model/task-progress';
import type { TaskProgress } from '@/types/models';

export function useTaskProgressModel(task: TaskProgress) {
  const { t, i18n } = useTranslation();
  const [now, setNow] = useState(Date.now);
  const active = task.state === 'running' || task.state === 'cancelling';
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  const number = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });
  const units = (value: number) => (task.unit === 'bytes' ? formatBytes(value) : number.format(value));
  const heartbeat = taskHeartbeatState(task, now);
  const status = t(`taskProgress.state.${task.state}`);
  const count =
    task.totalUnits === undefined && task.completedUnits === 0
      ? undefined
      : task.totalUnits === undefined
        ? units(task.completedUnits)
        : t('taskProgress.count', { processed: units(task.completedUnits), total: units(task.totalUnits) });
  const rate =
    !active || heartbeat !== 'current' || task.rate === undefined
      ? undefined
      : task.unit === 'bytes'
        ? t('taskProgress.bytesPerSecond', { amount: formatBytes(task.rate) })
        : t('taskProgress.itemsPerSecond', { amount: number.format(task.rate) });
  const eta =
    !active || heartbeat !== 'current'
      ? undefined
      : task.etaSeconds === undefined
        ? t('taskProgress.estimating')
        : t('taskProgress.stageRemaining', { duration: taskDuration(task.etaSeconds, i18n.language) });
  return {
    work: task.work ? t(`taskProgress.work.${task.work}`) : undefined,
    stage: taskStageLabel(task, task.stage, t),
    status,
    count,
    rate,
    eta,
    heartbeat: heartbeat === 'current' ? undefined : t(`taskProgress.heartbeat.${heartbeat}`),
    elapsed: taskDuration(task.elapsedMs / 1000, i18n.language),
    stages: task.stages.map((stage) => ({
      key: stage.key,
      weight: stage.weight,
      label: taskStageLabel(task, stage.key, t),
      state: t(`taskProgress.state.${stage.state}`),
      value: taskStageProgress(task, stage),
    })),
  };
}

export type TaskProgressModel = ReturnType<typeof useTaskProgressModel>;
