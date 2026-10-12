import { useTranslation } from 'react-i18next';
import { NavLink, useLocation } from 'react-router';
import { Button } from '@/app/components/ui/button';
import { Tooltip } from '@/app/components/ui/tooltip';
import { cn } from '@/app/components/ui/utils';
import { SidebarIcon, type SidebarIconName } from './SidebarIcon';
import type { NavigationLink, NavigationModel } from '@/features/shell/use-navigation-model';

const iconByPage: Record<NavigationLink['page'], SidebarIconName> = {
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

const railItemClass =
  'group relative flex size-11 items-center justify-center rounded-full text-forensics-muted transition-colors';
/** 可见的 32px 圆；外层 44px 保留点击面积。 */
const railDiscClass =
  'flex size-8 items-center justify-center rounded-full transition-colors group-hover:bg-forensics-hover group-hover:text-forensics-text group-focus-visible:ring-2 group-focus-visible:ring-forensics-red';
const railActiveDiscClass =
  'bg-forensics-rail-active text-forensics-red-ink group-hover:bg-forensics-rail-active group-hover:text-forensics-red-ink';

function RailItem({ link, model }: { link: NavigationLink; model: NavigationModel }) {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const label = t(`navigation.links.${link.page}.label`);
  // The active class is computed here instead of through NavLink's render-prop: the tooltip
  // clones this child, and a function className would be stringified by the clone.
  const isActive = link.to === '/' ? pathname === '/' : pathname.startsWith(link.to);
  return (
    <Tooltip content={label}>
      <NavLink
        to={link.to}
        onClick={() => model.selectPage(link.page)}
        aria-label={label}
        aria-current={isActive ? 'page' : undefined}
        className={cn(railItemClass, 'focus-visible:outline-none', isActive && 'text-forensics-red-ink')}
      >
        <span className={cn(railDiscClass, isActive && railActiveDiscClass)}>
          <SidebarIcon name={iconByPage[link.page]} selected={isActive} className="size-[21px]" />
        </span>
      </NavLink>
    </Tooltip>
  );
}

export function ActivityBar({
  model,
}: {
  model: NavigationModel;
}) {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const isSettings = pathname.startsWith('/settings');
  const jobsLabel = t('navigation.jobsRunning', { count: model.runningCount });
  return (
    <aside className="relative z-20 flex w-12 shrink-0 flex-col items-center bg-forensics-base" aria-label={t('navigation.appNavLabel')}>
      <nav className="flex min-h-0 flex-1 flex-col items-center gap-1.5 overflow-y-auto scrollbar-none px-1 pb-3 pt-4" aria-label={t('navigation.workspaceNavLabel')}>
        {model.links.map((link) => <RailItem key={link.to} link={link} model={model} />)}
      </nav>
      <div className="mt-2 flex w-full flex-col items-center gap-1.5 px-1 pb-3 pt-3">
        <Tooltip content={jobsLabel}>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            aria-label={jobsLabel}
            onClick={model.toggleDrawer}
            className={railItemClass}
          >
            <span className={railDiscClass}>
              <SidebarIcon name="jobs" className="size-[21px]" />
            </span>
            {model.runningCount > 0 ? <span className="absolute right-2 top-1.5 min-w-3 rounded-full bg-forensics-primary-blue px-1 text-center text-[9px] leading-3 text-white">{model.runningCount}</span> : null}
          </Button>
        </Tooltip>
        <Tooltip content={t('settings.title')}>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            aria-label={t('settings.title')}
            aria-current={isSettings ? 'page' : undefined}
            onClick={model.openSettings}
            className={railItemClass}
          >
            <span className={cn(railDiscClass, isSettings && railActiveDiscClass)}>
              <SidebarIcon name="settings" selected={isSettings} className="size-[21px]" />
            </span>
          </Button>
        </Tooltip>
      </div>
    </aside>
  );
}
