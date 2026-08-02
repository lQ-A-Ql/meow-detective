import { Activity, Search, Settings } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { HorizontalScroll } from '@/components/layout/HorizontalScroll';
import type { TopBarModel } from '@/features/shell/use-top-bar-model';

export function TopBar({ model }: { model: TopBarModel }) {
  const { t } = useTranslation();
  return (
    <div className="shrink-0 border-b border-forensics-border bg-forensics-panel px-6 py-3 text-xs">
      <div className="flex items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-6">
          <HorizontalScroll className="flex min-w-0 items-center gap-5">
            {model.links.map((link) => (
              <NavLink
                key={link.to}
                to={link.to}
                onClick={() => model.selectPage(link.page)}
                className={({ isActive }) =>
                  `whitespace-nowrap underline-offset-4 transition-colors hover:text-forensics-text hover:decoration-forensics-sakura-400 ${
                    isActive
                      ? 'font-light text-forensics-text underline decoration-forensics-sakura-500 decoration-1'
                      : 'text-forensics-muted'
                  }`
                }
              >
                {t(`topBar.links.${link.page}.label`)}
              </NavLink>
            ))}
          </HorizontalScroll>
          <div className="hidden min-w-0 border-l border-forensics-border pl-4 text-[11px] text-forensics-muted 2xl:block">
            <span className="block max-w-48 truncate">{model.currentCaseName ?? t('topBar.case.noCase')}</span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-3 text-forensics-muted">
          <div className="flex items-center gap-2 rounded-none border border-forensics-border bg-transparent px-2 py-1">
            <Search size={12} className="text-forensics-muted-light" />
            <Input
              value={model.globalSearchQuery}
              onChange={(event) => model.setGlobalSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') model.submitSearch();
              }}
              variant="search"
              inputSize="inline"
              className="w-40 font-mono text-xs xl:w-56"
              placeholder={t('topBar.search.placeholder')}
            />
          </div>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            onClick={model.toggleDrawer}
            title={t('topBar.jobs.running', { count: model.runningCount })}
            aria-label={t('topBar.jobs.running', { count: model.runningCount })}
            className="relative border border-transparent text-forensics-text-tertiary hover:border-forensics-border hover:bg-forensics-surface hover:text-forensics-text"
          >
            <Activity size={13} />
            {model.runningCount > 0 ? (
              <span className="absolute -right-1 -top-1 min-w-3 border border-forensics-panel bg-forensics-primary-blue px-0.5 text-center text-[9px] text-white">
                {model.runningCount}
              </span>
            ) : null}
          </Button>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            onClick={model.openSettings}
            title={t('settings.title')}
            aria-label={t('settings.title')}
            className="text-forensics-text-tertiary hover:bg-forensics-surface hover:text-forensics-text"
          >
            <Settings size={13} />
          </Button>
        </div>
      </div>
    </div>
  );
}
