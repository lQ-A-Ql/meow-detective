import { useTranslation } from 'react-i18next';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Checkbox } from '@/app/components/ui/checkbox';
import { DIGEST_ALGORITHMS } from '../digest-algorithms';
import type { DigestAlgorithm } from '@/types/digest';

export function DigestAlgorithmActions({ available, loading, forceRecompute = false, onForceRecomputeChange, onCalculate }: {
  available: boolean;
  loading: boolean;
  forceRecompute?: boolean;
  onForceRecomputeChange?: (value: boolean) => void;
  onCalculate: (algorithm: DigestAlgorithm) => void;
}) {
  const { t } = useTranslation();
  return <div className="space-y-2">
    {onForceRecomputeChange ? <label className="flex cursor-pointer items-center gap-2 text-[11px] text-forensics-muted">
      <Checkbox
        variant="forensics"
        checked={forceRecompute}
        disabled={!available || loading}
        onCheckedChange={(checked) => onForceRecomputeChange(checked === true)}
      />
      {t('fileBrowser.inspector.hash.forceRecompute')}
    </label> : null}
    <div className="grid grid-cols-2 gap-2">
    {DIGEST_ALGORITHMS.map(({ value, label }) => <div key={value} className="flex min-w-0 items-center gap-2">
      <IconButton label={t('fileBrowser.inspector.hash.run', { algorithm: label })}
        variant="forensicsOutline" disabled={!available || loading} onClick={() => onCalculate(value)}>
        <ActionIcon name="play" />
      </IconButton>
      <span className="font-mono text-[12px] text-forensics-ink">{label}</span>
    </div>)}
    </div>
  </div>;
}
