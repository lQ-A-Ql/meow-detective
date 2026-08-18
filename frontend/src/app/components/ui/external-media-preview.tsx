import { useState, type ImgHTMLAttributes } from 'react';
import { AlertTriangle, ImageOff } from 'lucide-react';
import { Badge } from './badge';
import { cn } from './utils';

export interface ExternalMediaPreviewProps extends Omit<ImgHTMLAttributes<HTMLImageElement>, 'src'> {
  sourceUrl: string;
  warningLabel: string;
  unavailableLabel: string;
  blockedLabel: string;
  mediaClassName?: string;
}

function allowedSourceUrl(sourceUrl: string): URL | undefined {
  try {
    const url = new URL(sourceUrl);
    if (!['http:', 'https:'].includes(url.protocol)) return undefined;
    const hostname = url.hostname.toLocaleLowerCase();
    if (hostname !== 'qpic.cn' && !hostname.endsWith('.qpic.cn')) return undefined;
    return url;
  } catch {
    return undefined;
  }
}

export function ExternalMediaPreview({
  sourceUrl,
  warningLabel,
  unavailableLabel,
  blockedLabel,
  alt = '',
  className,
  mediaClassName,
  onError,
  ...props
}: ExternalMediaPreviewProps) {
  const [failed, setFailed] = useState(false);
  const url = allowedSourceUrl(sourceUrl);
  const mediaClass = cn(
    'block max-w-full border border-forensics-border bg-forensics-panel object-contain',
    mediaClassName,
  );

  return (
    <div className={cn('min-w-0', className)}>
      <Badge variant="outline" className="mb-1 max-w-full gap-1 text-[9px] leading-4 whitespace-normal">
        <AlertTriangle />
        <span>{warningLabel}</span>
      </Badge>
      {!url ? (
        <div
          role="img"
          aria-label={alt || blockedLabel}
          className={cn(
            'flex min-h-20 items-center justify-center gap-2 border border-dashed border-forensics-border-strong bg-forensics-panel px-3 py-4 text-[10px] text-forensics-muted',
            mediaClassName,
          )}
        >
          <ImageOff className="size-4 shrink-0" />
          <span>{blockedLabel}</span>
        </div>
      ) : failed ? (
        <div
          role="img"
          aria-label={alt || unavailableLabel}
          className={cn(
            'flex min-h-20 items-center justify-center gap-2 border border-dashed border-forensics-border-strong bg-forensics-panel px-3 py-4 text-[10px] text-forensics-muted',
            mediaClassName,
          )}
        >
          <ImageOff className="size-4 shrink-0" />
          <span>{unavailableLabel}</span>
        </div>
      ) : (
        <img
          {...props}
          src={url.toString()}
          alt={alt}
          loading="lazy"
          decoding="async"
          referrerPolicy="no-referrer"
          className={mediaClass}
          data-media-source="external-unverified"
          onError={(event) => {
            setFailed(true);
            onError?.(event);
          }}
        />
      )}
    </div>
  );
}
