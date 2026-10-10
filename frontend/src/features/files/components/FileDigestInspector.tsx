import { useTranslation } from 'react-i18next';
import { Hash } from 'lucide-react';
import { IconButton } from '@/components/controls/IconButton';
import { InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import type { DigestAlgorithm, EvidenceDigest } from '@/types/digest';

const ALGORITHMS: DigestAlgorithm[] = ['md5', 'sha1', 'sha256', 'sm3'];

export function FileDigestInspector({ digest, loading, error, onCalculate }: { digest?: EvidenceDigest; loading: boolean; error?: string; onCalculate: (algorithm: DigestAlgorithm) => void }) {
  const { t } = useTranslation();
  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.hash')}>
      <div className="space-y-2">
        <div className="flex flex-wrap items-center gap-1">
          {ALGORITHMS.map((algorithm) => (
            <IconButton
              key={algorithm}
              label={algorithm.toUpperCase()}
              size="iconSm"
              variant="forensicsOutline"
              disabled={loading}
              onClick={() => onCalculate(algorithm)}
            >
              <Hash size={13} />
            </IconButton>
          ))}
        </div>
        {loading ? <InspectorValue value={t('fileBrowser.inspector.hash.loading')} /> : null}
        {error ? <div className="text-[10px] text-forensics-error-text">{error}</div> : null}
        {digest ? <div className="space-y-1 font-mono text-[10px]"><div>{digest.algorithm.toUpperCase()} · {digest.byteLength} bytes</div><div className="break-all text-forensics-text-secondary">{digest.value}</div></div> : null}
      </div>
    </InspectorSection>
  );
}
