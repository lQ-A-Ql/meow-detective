import { EyeOff } from 'lucide-react';

export function FileVisibilityToggle({
  checked,
  onCheckedChange,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <label className="inline-flex h-7 items-center gap-2 rounded border border-[#d8d8d8] bg-white px-2 text-[11px] text-[#555] hover:bg-[#f5f5f5]">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onCheckedChange(event.currentTarget.checked)}
        className="size-3 accent-[#111]"
        aria-label="toggle-hidden-files"
        data-testid="file-visibility-toggle"
      />
      <EyeOff size={12} />
      <span>显示隐藏文件</span>
    </label>
  );
}
