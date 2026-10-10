import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router';
import { Button } from '@/app/components/ui/button';
import { cn } from '@/app/components/ui/utils';
import meowMark from '@/assets/icons/sidebar/meow-mark.svg';
import { SidebarIcon, type SidebarIconName } from './SidebarIcon';
import type { TopBarLink, TopBarModel } from '@/features/shell/use-top-bar-model';

const iconByPage: Record<TopBarLink['page'], SidebarIconName> = {
  home: 'home',
  files: 'files',
  emulation: 'emulation',
  analysis: 'analysis',
  infrastructure: 'infrastructure',
  v2: 'v2',
  v3: 'v3',
  search: 'search',
  timeline: 'timeline',
  artifacts: 'artifacts',
  reports: 'reports',
};

function RailItem({ link, model }: { link: TopBarLink; model: TopBarModel }) {
  const { t } = useTranslation();
  const label = t(`topBar.links.${link.page}.label`);
  return (
    <NavLink
      to={link.to}
      onClick={() => model.selectPage(link.page)}
      aria-label={label}
      className={({ isActive }) => cn(
        'group relative flex h-11 w-14 items-center justify-center border-l-2 border-transparent text-forensics-muted transition-colors',
        'hover:bg-forensics-sakura-100 hover:text-forensics-sakura-600 focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-forensics-primary-blue/40',
        isActive && 'border-forensics-primary-blue bg-forensics-sakura-100 text-forensics-primary-blue',
      )}
    >
      <SidebarIcon name={iconByPage[link.page]} className="size-[21px]" />
      <span role="tooltip" className="pointer-events-none absolute left-[calc(100%+8px)] z-50 hidden whitespace-nowrap border border-forensics-border-strong bg-white px-2.5 py-1.5 text-[11px] text-forensics-text shadow-sm group-hover:block group-focus-visible:block">
        {label}
      </span>
    </NavLink>
  );
}

export function ActivityBar({ model }: { model: TopBarModel }) {
  const { t } = useTranslation();
  return (
    <aside className="relative z-20 flex w-14 shrink-0 flex-col items-center border-r border-forensics-border bg-white" aria-label={t('topBar.appNavLabel')}>
      <div className="flex h-12 w-full items-center justify-center border-b border-forensics-border">
        <div className="flex size-7 items-center justify-center border border-forensics-primary-blue text-forensics-primary-blue" aria-label="Meow Detective">
          <img src={meowMark} alt="" className="size-5" />
        </div>
      </div>
      <nav className="flex min-h-0 flex-1 flex-col items-center overflow-y-auto py-2" aria-label={t('topBar.workspaceNavLabel')}>
        {model.links.map((link) => <RailItem key={link.to} link={link} model={model} />)}
      </nav>
      <div className="flex w-full flex-col items-center border-t border-forensics-border py-2">
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          aria-label={t('topBar.jobs.running', { count: model.runningCount })}
          onClick={model.toggleDrawer}
          className="group relative flex h-11 w-14 items-center justify-center rounded-none border-l-2 border-transparent text-forensics-muted hover:bg-forensics-sakura-100 hover:text-forensics-sakura-600"
        >
          <SidebarIcon name="jobs" className="size-[21px]" />
          {model.runningCount > 0 ? <span className="absolute right-2 top-1.5 min-w-3 rounded-full bg-forensics-primary-blue px-1 text-center text-[9px] leading-3 text-white">{model.runningCount}</span> : null}
          <span role="tooltip" className="pointer-events-none absolute left-[calc(100%+8px)] z-50 hidden whitespace-nowrap border border-forensics-border-strong bg-white px-2.5 py-1.5 text-[11px] text-forensics-text shadow-sm group-hover:block group-focus-visible:block">{t('topBar.jobs.running', { count: model.runningCount })}</span>
        </Button>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          aria-label={t('settings.title')}
          onClick={model.openSettings}
          className="group relative flex h-11 w-14 items-center justify-center rounded-none border-l-2 border-transparent text-forensics-muted hover:bg-forensics-sakura-100 hover:text-forensics-sakura-600"
        >
          <SidebarIcon name="settings" className="size-[21px]" />
          <span role="tooltip" className="pointer-events-none absolute left-[calc(100%+8px)] z-50 hidden whitespace-nowrap border border-forensics-border-strong bg-white px-2.5 py-1.5 text-[11px] text-forensics-text shadow-sm group-hover:block group-focus-visible:block">{t('settings.title')}</span>
        </Button>
      </div>
    </aside>
  );
}
