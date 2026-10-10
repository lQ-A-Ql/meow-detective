import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/app/components/ui/tooltip';
import { Play } from 'lucide-react';
import { Link } from 'react-router';
import { Button } from '@/app/components/ui/button';

/** Navigation affordances shared by empty states, so no view hand-rolls its own. */
export function ImportDataAction() {
  const { t } = useTranslation();
  return (
    <Button asChild variant="outline" size="sm">
      <Link to="/">{t('common.emptyGoToImport')}</Link>
    </Button>
  );
}

export function RunAnalysisAction({ onClick, disabled = false }: { onClick: () => void; disabled?: boolean }) {
  const { t } = useTranslation();
  return (
    <Tooltip content={t('analysis.header.run')}><Button aria-label={t('analysis.header.run')} type="button" variant="outline" size="sm" onClick={onClick} disabled={disabled}><Play size={15} /></Button></Tooltip>
  );
}
