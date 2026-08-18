import { useState, type HTMLAttributes } from 'react';
import { ImageOff } from 'lucide-react';
import { SvgRenderer } from './svg-renderer';
import { cn } from './utils';

export interface InlineMediaData {
  mimeType: string;
  dataBase64: string;
  sha256?: string;
  sizeBytes?: number;
}

export interface InlineMediaProps extends HTMLAttributes<HTMLDivElement> {
  media: InlineMediaData;
  alt?: string;
  mediaClassName?: string;
}

export function InlineMedia({
  media,
  alt = '',
  className,
  mediaClassName,
  ...props
}: InlineMediaProps) {
  const [failed, setFailed] = useState(false);
  const source = `data:${media.mimeType};base64,${media.dataBase64}`;
  const sharedClassName = cn(
    'block max-w-full border border-forensics-border bg-forensics-panel object-contain',
    mediaClassName,
  );
  const unsupported = !media.mimeType.startsWith('image/')
    && !media.mimeType.startsWith('audio/')
    && !media.mimeType.startsWith('video/');
  return (
    <div className={cn('min-w-0', className)} {...props}>
      {failed || unsupported ? (
        <div
          role="img"
          aria-label={alt || media.mimeType}
          title={media.mimeType}
          className={cn(
            'flex min-h-20 items-center justify-center gap-2 border border-dashed border-forensics-border-strong bg-forensics-panel px-3 py-4 text-[10px] text-forensics-muted',
            mediaClassName,
          )}
        >
          <ImageOff className="size-4 shrink-0" />
          <span className="break-all">{media.mimeType}</span>
        </div>
      ) : null}
      {!failed && !unsupported && media.mimeType === 'image/svg+xml' ? (
        <SvgRenderer
          dataBase64={media.dataBase64}
          alt={alt}
          className={sharedClassName}
          onError={() => setFailed(true)}
        />
      ) : null}
      {!failed && !unsupported && media.mimeType.startsWith('image/') && media.mimeType !== 'image/svg+xml' ? (
        <img
          src={source}
          alt={alt}
          loading="lazy"
          className={sharedClassName}
          onError={() => setFailed(true)}
        />
      ) : null}
      {!failed && !unsupported && media.mimeType.startsWith('audio/') ? (
        <audio controls preload="metadata" src={source} className={cn('max-w-full', mediaClassName)} onError={() => setFailed(true)} />
      ) : null}
      {!failed && !unsupported && media.mimeType.startsWith('video/') ? (
        <video controls preload="metadata" src={source} className={sharedClassName} onError={() => setFailed(true)} />
      ) : null}
    </div>
  );
}
