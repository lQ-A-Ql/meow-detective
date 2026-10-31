import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Label } from '@/app/components/ui/label';
import { DIGEST_ALGORITHMS } from '@/lib/digest-algorithms';
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
    {onForceRecomputeChange ? <Label className="cursor-pointer text-[11px] text-forensics-muted">
      <Checkbox
        variant="forensics"
        checked={forceRecompute}
        disabled={!available || loading}
        onCheckedChange={(checked) => onForceRecomputeChange(checked === true)}
      />
      {t('fileBrowser.inspector.hash.forceRecompute')}
    </Label> : null}
    <div className="grid grid-cols-2 gap-1">
    {DIGEST_ALGORITHMS.map(({ value, label }) =>
      <Button key={value}
        shape="row"
        aria-label={t('fileBrowser.inspector.hash.run', { algorithm: label })}
        variant="forensicsOutline"
        disabled={!available || loading}
        onClick={() => onCalculate(value)}>
        <ActionIcon name="play" />
        <span className="min-w-0 truncate font-mono text-[11px] text-forensics-ink">{label}</span>
      </Button>
    )}
    </div>
  </div>;
}
