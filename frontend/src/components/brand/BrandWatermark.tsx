import { cn } from '@/app/components/ui/utils';
import { AssetSlot, type WatermarkArtwork } from '@/app/components/AssetSlot';

export type BrandWatermarkMotif = WatermarkArtwork;

export interface BrandWatermarkProps {
  motif: BrandWatermarkMotif;
  className?: string;
  layout?: 'motif' | 'banner';
}

export function BrandWatermark({ motif, className, layout = 'motif' }: BrandWatermarkProps) {
  if (layout === 'banner') {
    return (
      <div aria-hidden="true" data-slot="brand-banner" className={cn('brand-watermark brand-banner pointer-events-none select-none', className)}>
        <div className="brand-banner-edge brand-banner-edge-left">
          <AssetSlot role={motif} className="brand-banner-art" />
        </div>
        <div className="brand-banner-edge brand-banner-edge-right">
          <AssetSlot role={motif} className="brand-banner-art" />
        </div>
      </div>
    );
  }

  return (
    <AssetSlot role={motif} className={cn('brand-watermark', className)} />
  );
}
