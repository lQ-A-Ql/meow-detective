import { useTranslation } from 'react-i18next';
import { importProgressMetrics } from '@/features/jobs/model/import-progress-metrics';
import type { ImportPhaseMetrics } from '@/types/models';

export function ImportProgressMetrics({ metrics }: { metrics: ImportPhaseMetrics }) {
  const { t, i18n } = useTranslation();
  const values = importProgressMetrics(metrics, t, i18n.language);
  if (values.length === 0) return null;
  return (
    <dl className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[10px]">
      {values.map(({ label, value }) => (
        <div key={label}>
          <dt className="text-forensics-muted">{label}</dt>
          <dd className="font-mono tabular-nums text-forensics-text-secondary">{value}</dd>
        </div>
      ))}
    </dl>
  );
}
