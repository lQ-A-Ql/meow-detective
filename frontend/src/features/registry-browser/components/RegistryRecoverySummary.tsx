import { ScrollArea } from '@/app/components/ui/scroll-area';
import type { RegistryBrowserModel } from '../use-registry-browser-model';

export function RegistryRecoverySummary({ model }: { model: RegistryBrowserModel }) {
  const { t, recovery } = model;
  if (!recovery) return null;
  return <ScrollArea className="registry-recovery-summary" aria-label={t('registryBrowser.recoveryTitle')}>
    <div role="status" className="registry-recovery-heading">
      <strong>{t(`registryBrowser.recoveryStatuses.${recovery.status}`)}</strong>
      <span>{t('registryBrowser.replayCounts', { entries: recovery.appliedEntries, pages: recovery.appliedPages })}</span>
      <span>{t('registryBrowser.sequenceSummary', { primary: recovery.basePrimarySequence, secondary: recovery.baseSecondarySequence, result: recovery.resultSequence ?? t('registryBrowser.unknown') })}</span>
    </div>
    <div className="registry-recovery-logs">
      {recovery.logs.map((log) => <div key={log.slot}>
        <strong>{log.slot.toUpperCase()}</strong> <span>{t(`registryBrowser.logStatuses.${log.status}`)}</span>
        <code>{log.evidencePath}</code><code>{log.fileId}</code><code>{log.sha256}</code>
        {log.firstSequence !== undefined ? <span>{log.firstSequence}–{log.lastSequence}</span> : null}
        {log.diagnosticCode ? <span>{t(`registryBrowser.replayDiagnostics.${log.diagnosticCode}`, { defaultValue: t('registryBrowser.errors.recoveryFailed') })}</span> : null}
      </div>)}
    </div>
    {recovery.diagnosticCodes.map((code) => <p key={code}>{t(`registryBrowser.replayDiagnostics.${code}`, { defaultValue: t('registryBrowser.errors.recoveryFailed') })}</p>)}
    {recovery.baseSha256 ? <div className="registry-recovery-hash"><span>{t('registryBrowser.baseHash')}</span><code>{recovery.baseSha256}</code></div> : null}
    {recovery.resultSha256 ? <div className="registry-recovery-hash"><span>{t('registryBrowser.snapshotHash')}</span><code>{recovery.resultSha256}</code></div> : null}
  </ScrollArea>;
}
