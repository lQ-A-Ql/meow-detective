import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { REGISTRY_ANALYSIS_TABS } from '@/design/registry-analysis';
import type { RegistryExtractionSummary, RegistryHiveOverview, RegistryStructuredSummary } from '@/types/models';

type RegistryTab = typeof REGISTRY_ANALYSIS_TABS[number]['key'];
export function useRegistryPanelModel(summary?: RegistryExtractionSummary, structured?: RegistryStructuredSummary) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<RegistryTab>('users');
  const info = summary ?? { status: 'unavailable' as const, total: 0, values: [], generatedAt: '', warnings: [t('analysis.registry.unavailable')] };
  const data = structured;
  const hiveOverviews: RegistryHiveOverview[] = data?.hiveOverviews ?? [];
  const tabs = REGISTRY_ANALYSIS_TABS.map(({ key, labelKey, icon }) => ({ key, label: t(labelKey), icon }));
  const stats: Array<[string, string]> = hiveOverviews.length > 0
    ? hiveOverviews.map((hive) => [hive.hiveName, `${hive.keyValueCount} ${t('analysis.registry.values.entries')}${hive.txlogMerged ? ` · ${t('analysis.registry.values.txlog')}` : ''}${hive.deletedKeysFound > 0 ? ` · ${t('analysis.registry.values.deleted', { count: hive.deletedKeysFound })}` : ''}`])
    : [[t('analysis.registry.values.keyValues'), info.total.toString()], [t('analysis.registry.values.sourceHives'), new Set(info.values.map((value) => value.hivePath)).size.toString()]];
  const empty = (key: string) => ({ emptyTitle: t(`analysis.registry.empty.${key}Title`), emptyDescription: t(`analysis.registry.empty.${key}Description`) });

  return { t, activeTab, setActiveTab, info, data, tabs, stats, empty };
}
export type RegistryPanelModel = ReturnType<typeof useRegistryPanelModel>;
