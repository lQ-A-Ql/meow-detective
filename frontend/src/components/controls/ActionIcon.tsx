import { actionIconAssets, type ActionIconName } from '@/assets/icons/actions';

/**
 * Action artwork follows the same mechanism as the rail icons: a local SVG used as a CSS mask so
 * the button keeps tinting it with currentColor. The url() must stay quoted - Vite appends an
 * ?import query in dev and an unquoted token would drop the whole mask declaration.
 */
export function ActionIcon({ name, className = 'size-[15px]' }: { name: ActionIconName; className?: string }) {
  const mask = `url("${actionIconAssets[name]}")`;
  return (
    <span
      aria-hidden="true"
      className={`inline-block shrink-0 bg-current ${className}`}
      style={{
        maskImage: mask,
        WebkitMaskImage: mask,
        maskRepeat: 'no-repeat',
        WebkitMaskRepeat: 'no-repeat',
        maskPosition: 'center',
        WebkitMaskPosition: 'center',
        maskSize: 'contain',
        WebkitMaskSize: 'contain',
      }}
    />
  );
}
