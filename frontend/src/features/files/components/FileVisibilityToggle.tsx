import { Eye, EyeOff } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { IconButton } from '@/components/controls/IconButton';

/**
 * The hidden-file switch is a circular icon button, not a checkbox: the pressed state carries
 * "hidden entries are included", the hover hint carries the name.
 */
export function FileVisibilityToggle({
  checked,
  onCheckedChange,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <IconButton
      label={t('fileBrowser.status.showHidden')}
      pressed={checked}
      variant={checked ? 'forensicsSurface' : 'forensicsGhost'}
      onClick={() => onCheckedChange(!checked)}
      className="shrink-0"
      testId="file-visibility-toggle"
    >
      {checked ? <Eye size={15} /> : <EyeOff size={15} />}
    </IconButton>
  );
}
