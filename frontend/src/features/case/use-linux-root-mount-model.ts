import { useTranslation } from 'react-i18next';
import type { DataSourceSummary } from '@/types/models';
import { createLinuxRootMountViewModel } from './linux-root-mount-view-model';

export function useLinuxRootMountModel(source: DataSourceSummary) {
  const { t } = useTranslation();
  return createLinuxRootMountViewModel(source, t);
}
