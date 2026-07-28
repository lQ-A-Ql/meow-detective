import { cn } from '@/app/components/ui/utils';
import sittingCat from '@/assets/brand/watermark-sitting-cat.webp';
import walkingCat from '@/assets/brand/watermark-walking-cat.webp';
import documentPawCat from '@/assets/brand/watermark-document-paw.webp';

const watermarkSources = {
  sitting: sittingCat,
  walking: walkingCat,
  documentPaw: documentPawCat,
} as const;

export type BrandWatermarkMotif = keyof typeof watermarkSources;

export interface BrandWatermarkProps {
  motif: BrandWatermarkMotif;
  className?: string;
}

export function BrandWatermark({ motif, className }: BrandWatermarkProps) {
  return (
    <img
      src={watermarkSources[motif]}
      alt=""
      aria-hidden="true"
      draggable="false"
      className={cn('pointer-events-none select-none object-contain', className)}
    />
  );
}
