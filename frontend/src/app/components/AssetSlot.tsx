import manifest from '@/assets/anime/manifest.json';
import { watermarkArtwork, type WatermarkArtwork } from '@/assets/anime/watermarks';
import { cn } from '@/app/components/ui/utils';

export type AssetRole = 'brand-mark' | 'archive-scene' | 'investigator-portrait' | 'evidence-motif' | 'texture' | WatermarkArtwork;
export type { WatermarkArtwork };

const slots: Partial<Record<AssetRole, string | null>> = { ...manifest.slots, ...watermarkArtwork };

/**
 * The only way decoration enters the workbench. Supplied art never covers evidence: it is
 * inert, it is capped in contrast, and a missing file degrades to a quiet hatch instead of a
 * broken image.
 */
export function AssetSlot({ role, className, opacity }: { role: AssetRole; className?: string; opacity?: number }) {
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
      data-artwork={role}
      alt=""
      aria-hidden="true"
      loading="lazy"
      decoding="async"
      draggable="false"
      style={opacity === undefined ? undefined : { opacity }}
      className={cn('asset-slot pointer-events-none select-none object-contain', className)}
    />
  );
}
