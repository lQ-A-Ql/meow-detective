import { useTranslation } from 'react-i18next';
import type { BitLockerCatalogImportLifecycle } from '../hooks/use-bitlocker-volume';
import { useElapsedSeconds } from '../hooks/use-elapsed-seconds';
import { BitLockerCatalogImportOverlay } from '../components/BitLockerCatalogImportOverlay';

export function BitLockerCatalogImportOverlayController({ lifecycle }: { lifecycle: BitLockerCatalogImportLifecycle }) {
  const { t } = useTranslation();
  const elapsedSeconds = useElapsedSeconds(lifecycle.startedAt);
  const phaseLabel = lifecycle.phase === 'refreshing' ? t('fileBrowser.bitlockerImport.refreshing') : t('fileBrowser.bitlockerImport.cataloging');
  return <BitLockerCatalogImportOverlay lifecycle={lifecycle} elapsedSeconds={elapsedSeconds} phaseLabel={phaseLabel} titleLabel={t('fileBrowser.bitlockerImport.title')} />;
}
