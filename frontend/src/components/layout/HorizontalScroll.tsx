import {
  ComponentPropsWithoutRef,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { cn } from '@/app/components/ui/utils';
import './HorizontalScroll.css';

interface Metrics {
  scrollWidth: number;
  clientWidth: number;
  scrollLeft: number;
}

/**
 * HorizontalScroll - 自定义横向滚动容器
 *
 * 不依赖 ::-webkit-scrollbar 伪元素，完全用 DOM + CSS 实现：
 * - 1px 可见细线轨道
 * - 点光源高亮滑块
 * - 无原生箭头
 * - 支持鼠标拖拽滑块滚动
 */
export function HorizontalScroll({
  className,
  children,
  ...props
}: ComponentPropsWithoutRef<'div'>) {
  const innerRef = useRef<HTMLDivElement>(null);
  const frameRef = useRef<number | null>(null);
  const [metrics, setMetrics] = useState<Metrics>({
    scrollWidth: 0,
    clientWidth: 0,
    scrollLeft: 0,
  });
  const [dragging, setDragging] = useState(false);

  const update = useCallback(() => {
    const el = innerRef.current;
    if (!el) return;
    setMetrics({
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
      scrollLeft: el.scrollLeft,
    });
  }, []);

  const scheduleUpdate = useCallback(() => {
    if (frameRef.current != null) return;
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      update();
    });
  }, [update]);

  useEffect(() => {
    update();
    const el = innerRef.current;
    if (!el) return undefined;

    el.addEventListener('scroll', scheduleUpdate, { passive: true });
    const ro =
      typeof ResizeObserver !== 'undefined'
        ? new ResizeObserver(scheduleUpdate)
        : null;
    ro?.observe(el);
    window.addEventListener('resize', scheduleUpdate);

    return () => {
      el.removeEventListener('scroll', scheduleUpdate);
      ro?.disconnect();
      window.removeEventListener('resize', scheduleUpdate);
      if (frameRef.current != null) {
        cancelAnimationFrame(frameRef.current);
      }
    };
  }, [scheduleUpdate, update]);

  const trackWidth = metrics.clientWidth;
  const maxScroll = Math.max(0, metrics.scrollWidth - metrics.clientWidth);
  const thumbRatio =
    metrics.scrollWidth > 0 ? metrics.clientWidth / metrics.scrollWidth : 1;
  const thumbWidth = Math.max(32, trackWidth * thumbRatio);
  const thumbLeft =
    maxScroll > 0
      ? (metrics.scrollLeft / maxScroll) * (trackWidth - thumbWidth)
      : 0;
  const showThumb =
    metrics.scrollWidth > metrics.clientWidth && trackWidth > 0;

  const handleThumbMouseDown = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      event.preventDefault();
      setDragging(true);
      const startX = event.clientX;
      const startLeft = thumbLeft;

      const onMouseMove = (ev: MouseEvent) => {
        const delta = ev.clientX - startX;
        const available = trackWidth - thumbWidth;
        const newLeft = Math.min(Math.max(startLeft + delta, 0), available);
        const el = innerRef.current;
        if (el && available > 0) {
          el.scrollLeft = (newLeft / available) * maxScroll;
        }
      };

      const onMouseUp = () => {
        setDragging(false);
        window.removeEventListener('mousemove', onMouseMove);
        window.removeEventListener('mouseup', onMouseUp);
      };

      window.addEventListener('mousemove', onMouseMove);
      window.addEventListener('mouseup', onMouseUp);
    },
    [thumbLeft, trackWidth, thumbWidth, maxScroll],
  );

  return (
    <div className="scrollbar-thin-glow relative w-full min-w-0 flex-1">
      <div
        ref={innerRef}
        className={cn('scrollbar-thin-glow-scroll flex', className)}
        {...props}
      >
        {children}
      </div>
      <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-[7px]">
        <div className="scrollbar-thin-glow-track-line absolute inset-x-0 top-1/2 -translate-y-1/2" />
        {showThumb && (
          <div
            className={cn(
              'scrollbar-thin-glow-thumb pointer-events-auto absolute top-1/2 h-[7px] -translate-y-1/2 rounded-none cursor-pointer',
              dragging && 'cursor-grabbing',
            )}
            style={{ left: thumbLeft, width: thumbWidth }}
            onMouseDown={handleThumbMouseDown}
          />
        )}
      </div>
    </div>
  );
}
