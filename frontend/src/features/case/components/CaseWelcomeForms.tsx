import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/ui/tabs';
import { Tooltip } from '@/app/components/ui/tooltip';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { ArtisticTitle } from '@/components/typography/ArtisticTitle';
import { BRAND_DISPLAY_NAME } from '@/lib/branding';
import { caseWordmark } from '@/assets/wordmarks';
import type { CaseWelcomeState } from '../use-case-welcome-state';
import { RecentCasesDialog } from './RecentCasesDialog';
import type { CaseWelcomeFormsProps } from './case-welcome-types';

export function CaseWelcomeForms({ state, ...props }: CaseWelcomeFormsProps & { state: CaseWelcomeState }) {
  const { t } = useTranslation();
  return (
    <ScrollArea className="min-h-0 flex-1 bg-forensics-base" viewportClassName="case-welcome-viewport">
      <main className="case-welcome-stage">
        <div className="case-welcome-title-stage">
          <ArtisticTitle label={BRAND_DISPLAY_NAME} artwork={caseWordmark} />
        </div>
        <div className="case-welcome-entry-stage">
        <Tabs value={state.mode} onValueChange={state.selectMode} className="case-welcome-entry">
          <TabsList className="mx-auto h-9 gap-8 border-0" aria-label={t('caseHome.welcome.entryLabel')}>
            <TabsTrigger value="create" disabled={state.busy} className="px-0 text-[12px]">{t('caseHome.welcome.create.title')}</TabsTrigger>
            <TabsTrigger value="open" disabled={state.busy} className="px-0 text-[12px]">{t('caseHome.welcome.open.title')}</TabsTrigger>
          </TabsList>
          <TabsContent value="create" className="pt-3">
            <form onSubmit={state.submitCreate} aria-label={t('caseHome.welcome.create.title')} className="space-y-3">
              <Input value={props.caseName} onChange={(event) => props.setCaseName(event.target.value)} placeholder={t('caseHome.welcome.create.namePlaceholder')} aria-label={t('caseHome.welcome.create.namePlaceholder')} variant="line" disabled={state.busy} />
              <div className="flex items-center gap-3">
                <Input value={props.caseRoot} onChange={(event) => props.setCaseRoot(event.target.value)} placeholder={t('caseHome.welcome.create.rootPlaceholder')} aria-label={t('caseHome.welcome.create.rootPlaceholder')} variant="line" disabled={state.busy} />
                <Tooltip content={t('caseHome.welcome.create.action')}>
                  <Button type="submit" variant="solid" aria-label={t('caseHome.welcome.create.action')} disabled={!state.canCreate} aria-busy={props.createPending}><ActionIcon name="plus" /></Button>
                </Tooltip>
              </div>
              {props.createError ? <div role="alert" className="text-left text-[11px] text-forensics-error-text">{props.createError}</div> : null}
            </form>
          </TabsContent>
          <TabsContent value="open" className="pt-3">
            <form onSubmit={state.submitOpen} aria-label={t('caseHome.welcome.open.title')} className="space-y-3">
              <div className="flex items-center gap-3">
                <Input value={props.openCasePath} onChange={(event) => props.setOpenCasePath(event.target.value)} placeholder={t('caseHome.welcome.open.pathPlaceholder')} aria-label={t('caseHome.welcome.open.pathPlaceholder')} variant="line" disabled={state.busy} />
                <Tooltip content={t('caseHome.welcome.open.action')}>
                  <Button type="submit" variant="solid" aria-label={t('caseHome.welcome.open.action')} disabled={!state.canOpen} aria-busy={props.openPending}><ActionIcon name="folder-open" /></Button>
                </Tooltip>
              </div>
              {props.openError ? <div role="alert" className="text-left text-[11px] text-forensics-error-text">{props.openError}</div> : null}
            </form>
          </TabsContent>
        </Tabs>
        {props.recentCases.length ? (
          <Tooltip content={t('caseHome.welcome.recent.title')}>
            <Button type="button" variant="forensicsGhost" className="case-welcome-recent" aria-label={t('caseHome.welcome.recent.title')} onClick={() => state.setRecentOpen(true)} disabled={state.busy}><ActionIcon name="history" /></Button>
          </Tooltip>
        ) : null}
        </div>
        <RecentCasesDialog open={state.recentOpen} onOpenChange={state.setRecentOpen} recentCases={props.recentCases} onOpen={state.openRecent} onDelete={state.deleteRecent} />
      </main>
    </ScrollArea>
  );
}
