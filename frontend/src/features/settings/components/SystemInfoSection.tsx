import { useTranslation } from 'react-i18next';
import { BRAND_DISPLAY_NAME } from '@/lib/branding';

export function SystemInfoSection({ version }: { version?: string }) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <h2 className="text-[14px] text-forensics-ink">{t('settings.sections.systemInfo.title')}</h2>
      </div>
      <div className="max-w-4xl space-y-3 text-[12px] font-mono text-forensics-muted">
        <div className="flex justify-between border-b border-forensics-border-light pb-1">
          <span>{t('settings.sections.systemInfo.version')}</span>
          <span>{BRAND_DISPLAY_NAME} {version ?? t('appUpdate.unknown')}</span>
        </div>
        <div className="flex justify-between border-b border-forensics-border-light pb-1">
          <span>{t('settings.sections.systemInfo.platform')}</span>
          <span>{navigator.platform || 'Windows'}</span>
        </div>
        <div className="flex justify-between border-b border-forensics-border-light pb-1">
          <span>{t('settings.sections.systemInfo.database')}</span>
          <span>SQLite ({t('settings.sections.systemInfo.perCase')})</span>
        </div>
        <div className="flex justify-between border-b border-forensics-border-light pb-1">
          <span>{t('settings.sections.systemInfo.searchEngine')}</span>
          <span>Tantivy ({t('settings.sections.systemInfo.fullTextIndex')})</span>
        </div>
        <div className="flex justify-between border-b border-forensics-border-light pb-1">
          <span>{t('settings.sections.systemInfo.mcpProtocol')}</span>
          <span>v1.0 (Resources/Tools/Prompts)</span>
        </div>
      </div>
    </section>
  );
}
