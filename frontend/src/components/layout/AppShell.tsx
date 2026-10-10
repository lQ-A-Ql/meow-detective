import { PropsWithChildren, useMemo } from 'react';
import { useNavigate } from 'react-router';
import { BottomDrawerContainer } from '@/features/jobs/containers/BottomDrawerContainer';
import { ActivityBarContainer } from '@/features/shell/containers/ActivityBarContainer';
import { CommandPalette, type CommandPaletteItem } from '@/app/components/CommandPalette';
import { Toaster } from '@/app/components/ui/toast';
import { useNavigationModel } from '@/features/shell/use-navigation-model';
import { useTranslation } from 'react-i18next';

export function AppShell({ children }: PropsWithChildren) {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const model = useNavigationModel();
  const commands = useMemo<CommandPaletteItem[]>(() => [
    ...model.links.map((link) => ({
      id: 'go:' + link.to,
      label: t(`navigation.links.${link.page}.label`),
      group: t('commandPalette.group.goTo'),
      run: () => { model.selectPage(link.page); navigate(link.to); },
    })),
    { id: 'go:settings', label: t('settings.title'), group: t('commandPalette.group.goTo'), run: () => navigate('/settings') },
    { id: 'drawer', label: t('bottomDrawer.jobs.title'), group: t('commandPalette.group.view'), run: model.toggleDrawer },
  ], [model, navigate, t]);

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-background font-sans text-[13px] font-normal text-forensics-text selection:bg-forensics-sakura-100 selection:text-forensics-sakura-600">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <ActivityBarContainer />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">{children}</div>
      </div>
      <BottomDrawerContainer />
      <CommandPalette commands={commands} />
      <Toaster />
    </div>
  );
}
