import { cn } from '@/app/components/ui/utils';
import { AssetSlot, type WatermarkArtwork } from '@/app/components/AssetSlot';

export type BrandWatermarkMotif = WatermarkArtwork;

export interface BrandWatermarkProps {
  motif: BrandWatermarkMotif;
  className?: string;
}

export function BrandWatermark({ motif, className }: BrandWatermarkProps) {
  return (
    <AssetSlot role={motif} className={cn('brand-watermark', className)} />
  );
}
