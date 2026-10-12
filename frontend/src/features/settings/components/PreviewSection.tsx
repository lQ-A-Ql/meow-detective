import { useTranslation } from 'react-i18next';
import { Field, FieldHint, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import type { LocalSettings } from '@/lib/settings';

type PreviewKey = 'hexChunkBytes' | 'maxViewerRangeLength' | 'maxInlineImagePreviewBytes' | 'maxInlineMediaPreviewBytes';
interface Props extends Pick<LocalSettings, PreviewKey> {
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function PreviewSection(props: Props) {
  const { t } = useTranslation();
  const fields: PreviewKey[] = ['hexChunkBytes', 'maxViewerRangeLength', 'maxInlineImagePreviewBytes', 'maxInlineMediaPreviewBytes'];
  return (
    <div className="max-w-4xl space-y-5">
      <h2 className="mb-5 text-[14px] text-forensics-ink">{t('settings.sections.preview.title')}</h2>
      {fields.map((key) => (
        <Field key={key} layout="row">
          <FieldLabel htmlFor={`settings-preview-${key}`}>{t(`settings.sections.preview.${key}`)}</FieldLabel>
          <Input id={`settings-preview-${key}`} value={props[key]} inputMode="numeric" variant="numeric"
            onChange={(event) => props.setSettings((current) => ({ ...current, [key]: event.target.value }))} />
          <FieldHint>{t(`settings.sections.preview.${key}Hint`)}</FieldHint>
        </Field>
      ))}
    </div>
  );
}
