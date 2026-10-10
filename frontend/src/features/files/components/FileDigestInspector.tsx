import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { digestIconName } from '@/lib/digest-icon';
import type { DigestAlgorithm, EvidenceDigest } from '@/types/digest';

const ALGORITHMS: DigestAlgorithm[] = ['md5', 'sha1', 'sha256', 'sm3'];

/**
 * One row per algorithm: the drawn hash mark and the algorithm name, then the control that runs it.
 * The four rows sit in a 2x2 block so the whole digest choice is visible at once.
 */
export function FileDigestInspector({ digest, loading, error, onCalculate }: { digest?: EvidenceDigest; loading: boolean; error?: string; onCalculate: (algorithm: DigestAlgorithm) => void }) {
  const { t } = useTranslation();
  return (
    <InspectorSection title={t('fileBrowser.inspector.sections.hash')}>
      <div className="space-y-2">
        <div className="grid grid-cols-2 gap-1">
          {ALGORITHMS.map((algorithm) => (
            <div
              key={algorithm}
              className="flex min-w-0 items-center justify-between gap-1.5 border border-forensics-line bg-forensics-paper px-2 py-1"
            >
              <span className="flex min-w-0 items-center gap-1.5 font-mono text-[11px] text-forensics-ink-2">
                <ActionIcon name={digestIconName(algorithm)} className="size-[13px]" />
                <span className="truncate">{algorithm.toUpperCase()}</span>
              </span>
              <Tooltip content={t('fileBrowser.inspector.hash.run', { algorithm: algorithm.toUpperCase() })}>
                <Button
                  type="button"
                  variant="forensicsOutline"
                  aria-label={algorithm.toUpperCase()}
                  disabled={loading}
                  onClick={() => onCalculate(algorithm)}
                >
                  <ActionIcon name="play" />
                </Button>
              </Tooltip>
            </div>
          ))}
        </div>
        {loading ? <InspectorValue value={t('fileBrowser.inspector.hash.loading')} /> : null}
        {error ? <div className="text-[10px] text-forensics-error-text">{error}</div> : null}
        {digest ? <div className="space-y-1 font-mono text-[10px]"><div>{digest.algorithm.toUpperCase()} · {digest.byteLength} bytes</div><div className="break-all text-forensics-ink-2">{digest.value}</div></div> : null}
      </div>
    </InspectorSection>
  );
}
