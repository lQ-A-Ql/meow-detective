import manifest from '@/assets/anime/manifest.json';
import { cn } from '@/app/components/ui/utils';

export type AssetRole = 'brand-mark' | 'archive-scene' | 'investigator-portrait' | 'evidence-motif' | 'texture';

const slots = manifest.slots as Record<string, string | null>;

/**
 * The only way decoration enters the workbench. Supplied art never covers evidence: it is
 * inert, it is capped in contrast, and a missing file degrades to a quiet hatch instead of a
 * broken image.
 */
export function AssetSlot({ role, className, opacity = 0.08 }: { role: AssetRole; className?: string; opacity?: number }) {
  const source = slots[role];
  if (!source) {
    return (
      <div
        aria-hidden="true"
        className={cn('asset-slot-fallback pointer-events-none select-none', className)}
      />
    );
  }
  return (
    <img
      src={source}
      alt=""
      aria-hidden="true"
      loading="lazy"
      decoding="async"
      style={{ opacity }}
      className={cn('pointer-events-none select-none object-contain', className)}
    />
  );
}
