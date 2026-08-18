import type { ImgHTMLAttributes } from 'react';
import { cn } from './utils';

export interface SvgRendererProps extends Omit<ImgHTMLAttributes<HTMLImageElement>, 'src'> {
  dataBase64: string;
}

export function SvgRenderer({
  dataBase64,
  alt = '',
  className,
  ...props
}: SvgRendererProps) {
  return (
    <img
      src={`data:image/svg+xml;base64,${dataBase64}`}
      alt={alt}
      loading="lazy"
      draggable={false}
      className={cn('block max-w-full object-contain', className)}
      {...props}
    />
  );
}
