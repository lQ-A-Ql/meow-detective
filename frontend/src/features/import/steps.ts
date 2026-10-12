import type { Step } from './types';

export const STEP_ORDER: Step[] = ['platform', 'source', 'config', 'confirm'];

export const STEP_LABEL: Record<Step, string> = {
  platform: 'importDataSource.stepper.platform',
  source: 'importDataSource.stepper.source',
  config: 'importDataSource.stepper.config',
  confirm: 'importDataSource.stepper.confirm',
};

export const STEP_DESCRIPTION: Record<Step, string> = {
  platform: 'importDataSource.steps.platform',
  source: 'importDataSource.steps.source',
  config: 'importDataSource.steps.config',
  confirm: 'importDataSource.steps.confirm',
};
