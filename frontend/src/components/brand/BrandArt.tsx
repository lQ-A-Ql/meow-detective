import investigateArt from '@/assets/brand/investigate.png';
import linuxSourceArt from '@/assets/brand/data-source-linux.png';
import windowsSourceArt from '@/assets/brand/data-source-windows.png';
import { cn } from '@/app/components/ui/utils';

export type BrandArtVariant = 'investigate' | 'windows' | 'linux';

const brandArtByVariant: Record<BrandArtVariant, string> = {
  investigate: investigateArt,
  windows: windowsSourceArt,
  linux: linuxSourceArt,
};

const defaultAltByVariant: Record<BrandArtVariant, string> = {
  investigate: 'Meow~Detective investigation mascot',
  windows: 'Meow~Detective Windows data source mascot',
  linux: 'Meow~Detective Linux data source mascot',
};

export function BrandArt({
  variant,
  className,
  decorative = true,
  alt,
}: {
  variant: BrandArtVariant;
  className?: string;
  decorative?: boolean;
  alt?: string;
}) {
  return (
    <img
      src={brandArtByVariant[variant]}
      alt={decorative ? '' : alt ?? defaultAltByVariant[variant]}
      aria-hidden={decorative ? 'true' : undefined}
      draggable={false}
      className={cn('select-none object-contain', className)}
    />
  );
}
