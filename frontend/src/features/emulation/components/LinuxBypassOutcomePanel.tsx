import { useTranslation } from 'react-i18next';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { Badge } from '@/app/components/ui/badge';
import { Button } from '@/app/components/ui/button';
import type { EmulationWorkspaceModel } from '@/features/emulation/use-emulation-workspace-model';
import type {
  EmulationLinuxBypassAlternative,
  EmulationLinuxBypassBlocker,
  EmulationLinuxBypassMethod,
} from '@/types/models';

const BLOCKER_KEYS: Record<EmulationLinuxBypassBlocker, string> = {
  sharedExtents: 'emulationPage.bypassUnavailable.blockers.sharedExtents',
  dirtyLog: 'emulationPage.bypassUnavailable.blockers.dirtyLog',
  encryptedVolume: 'emulationPage.bypassUnavailable.blockers.encryptedVolume',
  unsupportedLayout: 'emulationPage.bypassUnavailable.blockers.unsupportedLayout',
  applianceUnavailable: 'emulationPage.bypassUnavailable.blockers.applianceUnavailable',
  accountNotFound: 'emulationPage.bypassUnavailable.blockers.accountNotFound',
  hashSchemeUnknown: 'emulationPage.bypassUnavailable.blockers.hashSchemeUnknown',
};

const METHOD_KEYS: Record<EmulationLinuxBypassMethod, string> = {
  hostInPlace: 'emulationPage.bypassUnavailable.methods.hostInPlace',
  linuxAppliance: 'emulationPage.bypassUnavailable.methods.linuxAppliance',
  none: 'emulationPage.bypassUnavailable.methods.none',
};

const ALTERNATIVE_KEYS: Record<EmulationLinuxBypassAlternative, string> = {
  launchWithoutBypass: 'emulationPage.bypassUnavailable.alternatives.launchWithoutBypass',
  externalRescueIso: 'emulationPage.bypassUnavailable.alternatives.externalRescueIso',
  retryAppliance: 'emulationPage.bypassUnavailable.alternatives.retryAppliance',
};

function translateEnum(record: Record<string, string>, code: string, t: (key: string) => string): string {
  const key = record[code];
  return key ? t(key) : code;
}

function booleanLabel(value: boolean | null | undefined, t: (key: string) => string): string {
  if (value === null || value === undefined) return t('emulationPage.bypassUnavailable.inspection.unknown');
  return value
    ? t('emulationPage.bypassUnavailable.inspection.yes')
    : t('emulationPage.bypassUnavailable.inspection.no');
}

export function LinuxBypassOutcomePanel({ model }: { model: EmulationWorkspaceModel }) {
  const { t } = useTranslation();
  const outcome = model.linuxBypassOutcome;
  if (!outcome) return null;
  const { result, inspection } = outcome;
  const rescueIsoReady = Boolean(inspection?.applianceAvailable);
  const manualLaunchDisabled = !rescueIsoReady || model.resumingBypass;
  const manualSteps = [1, 2, 3, 4, 5].map((n) => t(`emulationPage.bypassUnavailable.steps.step${n}`));

  return (
    <WorkspaceCard className="min-w-0 p-3">
      <fieldset className="space-y-3">
        <legend className="flex items-center gap-2 text-[12px] text-forensics-warning-text">
          <ActionIcon name="warning" className="size-4" />
          {t('emulationPage.bypassUnavailable.title')}
        </legend>
        <p className="text-[11px] leading-5 text-forensics-muted">
          {result.payloadStaged
            ? t('emulationPage.bypassUnavailable.payloadStaged')
            : t('emulationPage.bypassUnavailable.description')}
        </p>
        {result.reason ? (
          <div className="flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-forensics-muted">{t('emulationPage.bypassUnavailable.reasonLabel')}</span>
            <Badge variant="secondary">{translateEnum(BLOCKER_KEYS, result.reason, t)}</Badge>
          </div>
        ) : null}
        {inspection ? (
          <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-[11px]">
            <div>
              <dt className="text-forensics-muted">{t('emulationPage.bypassUnavailable.inspection.filesystem')}</dt>
              <dd className="font-mono text-forensics-text">{inspection.filesystem}</dd>
            </div>
            <div>
              <dt className="text-forensics-muted">
                {t('emulationPage.bypassUnavailable.inspection.recommendedMethod')}
              </dt>
              <dd className="text-forensics-text">{translateEnum(METHOD_KEYS, inspection.recommendedMethod, t)}</dd>
            </div>
            <div>
              <dt className="text-forensics-muted">{t('emulationPage.bypassUnavailable.inspection.volumeReflink')}</dt>
              <dd className="text-forensics-text">{booleanLabel(inspection.volumeReflinkEnabled, t)}</dd>
            </div>
            <div>
              <dt className="text-forensics-muted">
                {t('emulationPage.bypassUnavailable.inspection.targetInodeReflink')}
              </dt>
              <dd className="text-forensics-text">{booleanLabel(inspection.targetInodeReflink ?? null, t)}</dd>
            </div>
            <div>
              <dt className="text-forensics-muted">{t('emulationPage.bypassUnavailable.inspection.logClean')}</dt>
              <dd className="text-forensics-text">
                {inspection.logClean
                  ? t('emulationPage.bypassUnavailable.inspection.clean')
                  : t('emulationPage.bypassUnavailable.inspection.dirty')}
              </dd>
            </div>
            <div>
              <dt className="text-forensics-muted">
                {t('emulationPage.bypassUnavailable.inspection.applianceAvailable')}
              </dt>
              <dd className="text-forensics-text">{booleanLabel(inspection.applianceAvailable, t)}</dd>
            </div>
          </dl>
        ) : null}
        {result.alternatives.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {result.alternatives.map((alternative) => (
              <Badge key={alternative} variant="outline">
                {translateEnum(ALTERNATIVE_KEYS, alternative, t)}
              </Badge>
            ))}
          </div>
        ) : null}
        <div className="space-y-1">
          <p className="text-[11px] font-medium text-forensics-muted">
            {t('emulationPage.bypassUnavailable.stepsTitle')}
          </p>
          <ol className="list-decimal space-y-0.5 pl-5 text-[11px] leading-5 text-forensics-muted">
            {manualSteps.map((step, index) => (
              <li key={index}>{step}</li>
            ))}
          </ol>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            shape="pill"
            variant="solid"
            disabled={manualLaunchDisabled}
            title={rescueIsoReady ? undefined : t('emulationPage.bypassUnavailable.retryApplianceDisabled')}
            onClick={() => void model.retryBypassWithAppliance()}
          >
            <ActionIcon name="play" />
            {t('emulationPage.bypassUnavailable.actions.retryAppliance')}
          </Button>
          <Button
            type="button"
            shape="pill"
            variant="outline"
            disabled={model.resumingBypass}
            onClick={() => void model.launchWithoutBypass()}
          >
            <ActionIcon name="play" />
            {t('emulationPage.bypassUnavailable.actions.launchWithoutBypass')}
          </Button>
          <Button
            type="button"
            shape="pill"
            variant="outline"
            disabled={model.resumingBypass}
            onClick={() => void model.chooseExternalRescueIso()}
          >
            <ActionIcon name="folder-open" />
            {t('emulationPage.bypassUnavailable.actions.externalRescueIso')}
          </Button>
          <Button
            type="button"
            shape="pill"
            variant="forensicsDangerGhost"
            disabled={model.resumingBypass}
            onClick={() => void model.releasePreparedBypassSession()}
          >
            <ActionIcon name="close" />
            {t('emulationPage.bypassUnavailable.actions.release')}
          </Button>
        </div>
      </fieldset>
    </WorkspaceCard>
  );
}
