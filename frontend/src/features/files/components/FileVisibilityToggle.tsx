import { EyeOff } from 'lucide-react';
import { Checkbox } from '@/app/components/ui/checkbox';

export function FileVisibilityToggle({
  checked,
  onCheckedChange,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <label className="inline-flex h-7 items-center gap-2 rounded-none border border-forensics-border-strong bg-forensics-surface px-2 text-[11px] text-forensics-text-tertiary hover:bg-forensics-panel-strong">
      <Checkbox
        checked={checked}
        onCheckedChange={(value) => onCheckedChange(value === true)}
        variant="forensics"
        checkboxSize="compact"
        aria-label="toggle-hidden-files"
        data-testid="file-visibility-toggle"
      />
      <EyeOff size={12} />
      <span>显示隐藏文件</span>
    </label>
  );
}
