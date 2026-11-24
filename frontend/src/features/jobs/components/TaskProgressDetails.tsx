import { useTranslation } from 'react-i18next';
import { Progress } from '@/app/components/ui/progress';
import type { TaskProgressModel } from '@/features/jobs/hooks/use-task-progress-model';

export function TaskProgressDetails({ model }: { model: TaskProgressModel }) {
  const { t } = useTranslation();
  return (
    <div className="mt-2">
      <div role="status" className="text-forensics-text-secondary">
        {model.stage} · {model.status}
      </div>
      {model.work && model.work !== model.stage ? <div className="mt-1 text-forensics-muted">{model.work}</div> : null}
      <ol aria-label={t('taskProgress.stages')} className="mt-2 flex gap-1">
        {model.stages.map((stage) => (
          <li key={stage.key} className="min-w-0" style={{ flex: stage.weight }}>
            <Progress
              value={stage.value}
              indeterminate={stage.value === null}
              aria-label={stage.label}
              aria-valuetext={stage.state}
            />
            <span className="mt-1 block truncate text-[9px] text-forensics-muted">{stage.label}</span>
          </li>
        ))}
      </ol>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 font-mono tabular-nums text-forensics-text-secondary">
        {model.count ? <span>{model.count}</span> : null}
        {model.rate ? <span>{model.rate}</span> : null}
        <span>
          {t('taskProgress.elapsed')} {model.elapsed}
        </span>
      </div>
      {model.eta ? <div className="mt-1 text-forensics-muted">{model.eta}</div> : null}
      {model.heartbeat ? (
        <div role="status" className="mt-1 text-forensics-warning-text">
          {model.heartbeat}
        </div>
      ) : null}
    </div>
  );
}
