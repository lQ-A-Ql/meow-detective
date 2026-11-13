import { useTranslation } from 'react-i18next';

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
