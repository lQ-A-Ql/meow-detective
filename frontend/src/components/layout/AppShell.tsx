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
    <div className="relative flex h-screen flex-col overflow-hidden bg-forensics-base font-sans text-[13px] font-normal text-forensics-ink selection:bg-forensics-red-tint selection:text-forensics-red-deep-ink">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* 侧栏通高：底部横条只在工作区列内展开。 */}
        <ActivityBarContainer />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
          {/* 工作区大卡片：仅左下 20px 圆角；左上与右侧齐边全覆盖。 */}
          <div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-bl-shell border border-b-0 border-r-0 border-forensics-line bg-forensics-paper">
            {children}
          </div>
          <BottomDrawerContainer />
        </div>
      </div>
      <CommandPalette commands={commands} />
      <Toaster />
    </div>
  );
}
