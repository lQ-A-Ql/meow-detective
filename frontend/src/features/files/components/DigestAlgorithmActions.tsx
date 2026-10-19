import { useTranslation } from 'react-i18next';
import { IconButton } from '@/components/controls/IconButton';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { DIGEST_ALGORITHMS } from '../digest-algorithms';
import type { DigestAlgorithm } from '@/types/digest';

export function DigestAlgorithmActions({ available, loading, onCalculate }: {
  available: boolean;
  loading: boolean;
  onCalculate: (algorithm: DigestAlgorithm) => void;
}) {
  const { t } = useTranslation();
  return <div className="grid grid-cols-2 gap-2">
    {DIGEST_ALGORITHMS.map(({ value, label }) => <div key={value} className="flex min-w-0 items-center gap-2">
      <IconButton label={t('fileBrowser.inspector.hash.run', { algorithm: label })}
        variant="forensicsOutline" disabled={!available || loading} onClick={() => onCalculate(value)}>
        <ActionIcon name="play" />
      </IconButton>
      <span className="font-mono text-[12px] text-forensics-ink">{label}</span>
    </div>)}
  </div>;
}
