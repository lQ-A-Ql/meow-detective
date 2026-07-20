import { useTranslation } from 'react-i18next';
import { FieldHint } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
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
        <span className="text-[13px] font-light text-forensics-text-secondary">{t('settings.sections.preview.title')}</span>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-light text-forensics-text-tertiary">{t('settings.sections.preview.hexChunkBytes')}</span>
          <Input
            value={hexChunkBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                hexChunkBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            variant="numeric"
            inputSize="compact"
            className="mt-2"
          />
          <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.hexChunkBytesHint')}</FieldHint>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-light text-forensics-text-tertiary">{t('settings.sections.preview.maxViewerRangeLength')}</span>
          <Input
            value={maxViewerRangeLength}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxViewerRangeLength: event.target.value,
              }))
            }
            inputMode="numeric"
            variant="numeric"
            inputSize="compact"
            className="mt-2"
          />
          <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.maxViewerRangeLengthHint')}</FieldHint>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-light text-forensics-text-tertiary">{t('settings.sections.preview.maxInlineImagePreviewBytes')}</span>
          <Input
            value={maxInlineImagePreviewBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxInlineImagePreviewBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            variant="numeric"
            inputSize="compact"
            className="mt-2"
          />
          <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.maxInlineImagePreviewBytesHint')}</FieldHint>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-light text-forensics-text-tertiary">{t('settings.sections.preview.maxInlineMediaPreviewBytes')}</span>
          <Input
            value={maxInlineMediaPreviewBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxInlineMediaPreviewBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            variant="numeric"
            inputSize="compact"
            className="mt-2"
          />
          <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">{t('settings.sections.preview.maxInlineMediaPreviewBytesHint')}</FieldHint>
        </label>
      </div>
    </section>
  );
}
