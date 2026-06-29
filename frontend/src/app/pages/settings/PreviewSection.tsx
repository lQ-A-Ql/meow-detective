import { useTranslation } from 'react-i18next';
import type { LocalSettings } from '@/lib/settings';

interface PreviewSectionProps {
  hexChunkBytes: string;
  maxViewerRangeLength: string;
  maxInlineImagePreviewBytes: string;
  maxInlineMediaPreviewBytes: string;
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function PreviewSection({
  hexChunkBytes,
  maxViewerRangeLength,
  maxInlineImagePreviewBytes,
  maxInlineMediaPreviewBytes,
  setSettings,
}: PreviewSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-semibold text-forensics-text-secondary">{t('settings.sections.preview.title')}</span>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.preview.hexChunkBytes')}</span>
          <input
            value={hexChunkBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                hexChunkBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.hexChunkBytesHint')}</span>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.preview.maxViewerRangeLength')}</span>
          <input
            value={maxViewerRangeLength}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxViewerRangeLength: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.maxViewerRangeLengthHint')}</span>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.preview.maxInlineImagePreviewBytes')}</span>
          <input
            value={maxInlineImagePreviewBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxInlineImagePreviewBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.maxInlineImagePreviewBytesHint')}</span>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.preview.maxInlineMediaPreviewBytes')}</span>
          <input
            value={maxInlineMediaPreviewBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxInlineMediaPreviewBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.maxInlineMediaPreviewBytesHint')}</span>
        </label>
      </div>
    </section>
  );
}
