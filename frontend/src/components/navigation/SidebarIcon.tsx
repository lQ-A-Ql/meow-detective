import { sidebarIconAssets, sidebarSelectedIconAssets } from '@/assets/icons/sidebar';

export type SidebarIconName =
  | 'home'
  | 'files'
  | 'emulation'
  | 'analysis'
  | 'infrastructure'
  | 'v2'
  | 'v3'
  | 'search'
  | 'timeline'
  | 'artifacts'
  | 'reports'
  | 'settings'
  | 'jobs';

/**
 * The activity bar icons are local SVG assets used as CSS masks, so the artwork stays in the
 * asset folder and the rail can tint it with currentColor.
 *
 * The url() value MUST be quoted: Vite appends a `?import` query to dev asset URLs, and an
 * unquoted CSS <url> token cannot contain "?", so the browser would drop the whole
 * mask-image declaration and the span would paint its background as a solid block.
 */
export function SidebarIcon({ name, selected = false, className = '' }: { name: SidebarIconName; selected?: boolean; className?: string }) {
  const selectedAsset = selected ? sidebarSelectedIconAssets[name] : undefined;
  const mask = `url("${selectedAsset ?? sidebarIconAssets[name]}")`;
  return (
    <span
      aria-hidden="true"
      data-slot="sidebar-icon"
      data-icon={name}
      data-inverse={selectedAsset ? 'true' : undefined}
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
