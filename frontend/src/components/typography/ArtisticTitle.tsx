import { cn } from '@/app/components/ui/utils';
import type { CSSProperties } from 'react';

export function ArtisticTitle({ label, artwork, className }: {
  label: string;
  artwork: { src: string; viewBox: string; strokes: readonly string[] };
  className?: string;
}) {
  return (
    <h1 aria-label={label} className={cn('artistic-title', className)}>
      <svg aria-hidden="true" viewBox={artwork.viewBox} className="artistic-title__wordmark">
        {artwork.strokes.map((id, index) => <use key={id} href={`${artwork.src}#${id}`} style={{ '--stroke-index': index } as CSSProperties} />)}
      </svg>
    </h1>
  );
}
