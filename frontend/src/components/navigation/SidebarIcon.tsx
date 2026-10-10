import { sidebarIconAssets } from '@/assets/icons/sidebar';

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
 * The activity bar icons are local SVG assets. Keeping the artwork in the
 * asset folder makes the rail independent from icon fonts and third-party
 * icon geometry while this component only handles presentation.
 */
export function SidebarIcon({ name, className = '' }: { name: SidebarIconName; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={`inline-block shrink-0 bg-current ${className}`}
      style={{ maskImage: `url(${sidebarIconAssets[name]})`, WebkitMaskImage: `url(${sidebarIconAssets[name]})`, maskRepeat: 'no-repeat', WebkitMaskRepeat: 'no-repeat', maskPosition: 'center', WebkitMaskPosition: 'center', maskSize: 'contain', WebkitMaskSize: 'contain' }}
    />
  );
}
