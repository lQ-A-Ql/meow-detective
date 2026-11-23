import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { EmulationWorkspaceModel } from './use-emulation-workspace-model';
import type {
  EmulationLinuxBypassBlocker,
  EmulationLinuxBypassInspection,
  EmulationLinuxBypassMethod,
  EmulationLinuxBypassResult,
} from '@/types/models';

const BLOCKER_KEYS: Record<EmulationLinuxBypassBlocker, string> = {
  sharedExtents: 'emulationPage.bypassUnavailable.blockers.sharedExtents',
  dirtyLog: 'emulationPage.bypassUnavailable.blockers.dirtyLog',
  encryptedVolume: 'emulationPage.bypassUnavailable.blockers.encryptedVolume',
  unsupportedLayout: 'emulationPage.bypassUnavailable.blockers.unsupportedLayout',
  accountNotFound: 'emulationPage.bypassUnavailable.blockers.accountNotFound',
  hashSchemeUnknown: 'emulationPage.bypassUnavailable.blockers.hashSchemeUnknown',
};

const METHOD_KEYS: Record<EmulationLinuxBypassMethod, string> = {
  hostInPlace: 'emulationPage.bypassUnavailable.methods.hostInPlace',
  none: 'emulationPage.bypassUnavailable.methods.none',
};

export interface LinuxBypassOutcomePresentation {
  title: string;
  description: string;
  reasonLabel: string;
  reason?: string;
  details: { key: string; label: string; value: string }[];
  launchLabel: string;
  releaseLabel: string;
}

type Translate = (key: string) => string;

function booleanLabel(value: boolean | null | undefined, t: Translate): string {
  const state = value === null || value === undefined ? 'unknown' : value ? 'yes' : 'no';
  return t(`emulationPage.bypassUnavailable.inspection.${state}`);
}

export function presentLinuxBypassOutcome(
  result: EmulationLinuxBypassResult,
  inspection: EmulationLinuxBypassInspection | undefined,
  t: Translate,
): LinuxBypassOutcomePresentation {
  const values: [string, string][] = inspection
    ? [
        ['filesystem', inspection.filesystem],
        ['recommendedMethod', t(METHOD_KEYS[inspection.recommendedMethod])],
        ['volumeReflink', booleanLabel(inspection.volumeReflinkEnabled, t)],
        ['targetInodeReflink', booleanLabel(inspection.targetInodeReflink, t)],
        ['logClean', t(`emulationPage.bypassUnavailable.inspection.${inspection.logClean ? 'clean' : 'dirty'}`)],
      ]
    : [];
  return {
    title: t('emulationPage.bypassUnavailable.title'),
    description: t('emulationPage.bypassUnavailable.description'),
    reasonLabel: t('emulationPage.bypassUnavailable.reasonLabel'),
    reason: result.reason ? t(BLOCKER_KEYS[result.reason]) : undefined,
    details: values.map(([key, value]) => ({
      key,
      label: t(`emulationPage.bypassUnavailable.inspection.${key}`),
      value,
    })),
    launchLabel: t('emulationPage.bypassUnavailable.actions.launchWithoutBypass'),
    releaseLabel: t('emulationPage.bypassUnavailable.actions.release'),
  };
}

export function useLinuxBypassOutcomePanelModel(model: EmulationWorkspaceModel) {
  const { t } = useTranslation();
  const presentation = useMemo(() => {
    const outcome = model.linuxBypassOutcome;
    return outcome ? presentLinuxBypassOutcome(outcome.result, outcome.inspection, t) : undefined;
  }, [model.linuxBypassOutcome, t]);
  return {
    presentation,
    busy: model.bypassOutcomeBusy,
    onLaunch: model.launchWithoutBypass,
    onRelease: model.releasePreparedBypassSession,
  };
}
