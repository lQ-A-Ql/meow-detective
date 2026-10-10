import { ArrowLeft, Search } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useLocation } from 'react-router';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { SidebarIcon, type SidebarIconName } from '@/components/navigation/SidebarIcon';
import type { TopBarModel } from '@/features/shell/use-top-bar-model';

const iconByPage: Record<TopBarModel['links'][number]['page'], SidebarIconName> = {
  home: 'home', files: 'files', emulation: 'emulation', analysis: 'analysis',
  infrastructure: 'infrastructure', v2: 'v2', v3: 'v3', search: 'search',
  timeline: 'timeline', artifacts: 'artifacts', reports: 'reports',
};

export function TopBar({ model }: { model: TopBarModel }) {
  const { t } = useTranslation();
  const location = useLocation();
  const activeLink = model.links.find((link) => location.pathname === link.to)
    ?? model.links.find((link) => link.to !== '/' && location.pathname.startsWith(link.to))
    ?? model.links[0];
  const pageLabel = activeLink ? t(`topBar.links.${activeLink.page}.label`) : t('topBar.currentPage');
  const pageContext = activeLink ? t(`topBar.links.${activeLink.page}.context`) : '';

  return (
    <header className="flex min-h-14 shrink-0 items-center justify-between gap-5 border-b border-forensics-border bg-white px-5 py-2.5" aria-label={t('topBar.workspaceLabel')}>
      <div className="flex min-w-0 items-center gap-3">
        {activeLink ? <SidebarIcon name={iconByPage[activeLink.page]} className="size-5 text-forensics-primary-blue" /> : null}
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-[13px] font-semibold text-forensics-text">
            <span className="truncate">{pageLabel}</span>
            {model.currentCaseName ? <><span className="text-forensics-border-strong">/</span><span className="max-w-56 truncate font-normal text-forensics-muted">{model.currentCaseName}</span></> : null}
          </div>
          <div className="hidden truncate text-[10px] text-forensics-muted-light sm:block">{pageContext}</div>
        </div>
        <nav className="sr-only" aria-label={t('topBar.pageNavLabel')}>
          {model.links.filter((link) => link.to !== activeLink?.to).map((link) => <span key={link.to}>{t(`topBar.links.${link.page}.label`)}</span>)}
        </nav>
      </div>
      <div className="flex shrink-0 items-center gap-2 text-forensics-muted">
        {model.hasCurrentCase ? <Button type="button" variant="forensicsOutline" size="xs" onClick={model.returnToCaseEntry}><ArrowLeft size={13} />{t('topBar.case.return')}</Button> : null}
        <div className="flex items-center gap-2 border border-forensics-border bg-forensics-panel px-2 py-1 focus-within:border-forensics-primary-blue">
          <Search size={12} className="text-forensics-muted-light" />
          <Input
            value={model.globalSearchQuery}
            onChange={(event) => model.setGlobalSearchQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') model.submitSearch();
            }}
            variant="search"
            inputSize="inline"
            className="w-36 font-mono text-xs xl:w-52"
            placeholder={t('topBar.search.placeholder')}
            aria-label={t('topBar.search.placeholder')}
          />
        </div>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={model.toggleDrawer}
          title={t('topBar.jobs.running', { count: model.runningCount })}
          aria-label={t('topBar.jobs.running', { count: model.runningCount })}
          className="relative hidden sm:inline-flex"
        >
          <SidebarIcon name="jobs" className="size-4" />
          {model.runningCount > 0 ? <span className="absolute -right-1 -top-1 min-w-3 rounded-full bg-forensics-primary-blue px-1 text-center text-[9px] leading-3 text-white">{model.runningCount}</span> : null}
        </Button>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={model.openSettings}
          title={t('settings.title')}
          aria-label={t('settings.title')}
          className="hidden sm:inline-flex"
        >
          <SidebarIcon name="settings" className="size-4" />
        </Button>
      </div>
    </header>
  );
}
