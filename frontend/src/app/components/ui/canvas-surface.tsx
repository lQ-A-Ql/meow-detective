import * as React from 'react';

import { cn } from './utils';

interface CanvasSurfaceProps extends React.CanvasHTMLAttributes<HTMLCanvasElement> {
  containerClassName?: string;
}

const CanvasSurface = React.forwardRef<HTMLCanvasElement, CanvasSurfaceProps>(function CanvasSurface(
  { className, containerClassName, ...props },
  ref,
) {
  return (
    <div data-slot="canvas-surface" className={cn('relative w-full overflow-auto', containerClassName)}>
      <canvas ref={ref} data-slot="canvas" className={cn('block', className)} {...props} />
    </div>
  );
});

export { CanvasSurface };
