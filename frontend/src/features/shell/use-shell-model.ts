import { useMemo } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';
import { useCurrentCase } from '@/features/case/hooks';
import type { CommandPaletteItem } from '@/app/components/CommandPalette';
import { useNavigationModel } from './use-navigation-model';

export function useShellModel() {
  const location = useLocation();
  const navigate = useNavigate();
  const currentCase = useCurrentCase();
  const navigation = useNavigationModel();
  const { t } = useTranslation();
  const isCaseEntry = location.pathname === '/' && !currentCase.data;
  const commands = useMemo<CommandPaletteItem[]>(() => [
    ...navigation.links.map((link) => ({
      id: `go:${link.to}`,
      label: t(`navigation.links.${link.page}.label`),
      group: t('commandPalette.group.goTo'),
      run: () => { navigation.selectPage(link.page); navigate(link.to); },
    })),
    { id: 'go:settings', label: t('settings.title'), group: t('commandPalette.group.goTo'), run: () => navigate('/settings') },
    ...(!isCaseEntry ? [{ id: 'drawer', label: t('bottomDrawer.jobs.title'), group: t('commandPalette.group.view'), run: navigation.toggleDrawer }] : []),
  ], [isCaseEntry, navigate, navigation, t]);
  return { isCaseEntry, commands };
}
