/**
 * ImageViewer - 图片预览组件
 *
 * 功能：
 * - 鼠标滚轮缩放
 * - 拖拽平移
 * - 旋转
 * - 适应窗口
 * - 图片信息显示
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import {
  ZoomIn,
  ZoomOut,
  RotateCw,
  Maximize,
  Download,
  Image,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';

interface ImageViewerProps {
  /** 图片 URL (data: URL 或 blob: URL) */
  src: string;
  /** 图片 MIME 类型 */
  mimeType?: string;
  /** 文件名 */
  fileName?: string;
}

export function ImageViewer({ src, mimeType, fileName }: ImageViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  const [scale, setScale] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [imageSize, setImageSize] = useState({ width: 0, height: 0 });
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Reset state when switching between preview sources.
  useEffect(() => {
    setScale(1);
    setRotation(0);
    setPosition({ x: 0, y: 0 });
    setImageSize({ width: 0, height: 0 });
    setIsLoading(true);
    setError(null);
  }, [src]);

  // 图片加载完成
  const handleLoad = useCallback(() => {
    if (imgRef.current) {
      setImageSize({
        width: imgRef.current.naturalWidth,
        height: imgRef.current.naturalHeight,
      });
      setIsLoading(false);
      setError(null);
    }
  }, []);

  // 图片加载错误
  const handleError = useCallback((e: React.SyntheticEvent<HTMLImageElement, Event>) => {
    setIsLoading(false);
    const target = e.target as HTMLImageElement;
    
    // 提供更详细的错误信息
    if (!target.src || target.src === 'about:blank') {
      setError('图片源无效');
    } else if (target.src.startsWith('asset://') && !target.complete) {
      setError('图片加载超时，请检查文件是否存在');
    } else {
      setError('图片格式不支持或文件损坏');
    }
  }, []);

  // 鼠标滚轮缩放
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setScale((s) => Math.max(0.1, Math.min(10, s * delta)));
  }, []);

  // 鼠标拖拽开始
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (e.button === 0) {
        setIsDragging(true);
        setDragStart({
          x: e.clientX - position.x,
          y: e.clientY - position.y,
        });
      }
    },
    [position]
  );

  // 鼠标拖拽中
  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (isDragging) {
        const containerWidth = containerRef.current?.clientWidth ?? 0;
        const containerHeight = containerRef.current?.clientHeight ?? 0;
        const imageWidth = imageSize.width * scale;
        const imageHeight = imageSize.height * scale;
        const newX = e.clientX - dragStart.x;
        const newY = e.clientY - dragStart.y;
        setPosition({
          x: Math.max(Math.min(newX, containerWidth - 100), -(imageWidth - 100)),
          y: Math.max(Math.min(newY, containerHeight - 100), -(imageHeight - 100)),
        });
      }
    },
    [isDragging, dragStart, imageSize, scale]
  );

  // 鼠标拖拽结束
  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // 适应窗口
  const fitToWindow = useCallback(() => {
    if (containerRef.current && imageSize.width > 0) {
      const containerWidth = containerRef.current.clientWidth - 40;
      const containerHeight = containerRef.current.clientHeight - 40;
      const scaleX = containerWidth / imageSize.width;
      const scaleY = containerHeight / imageSize.height;
      setScale(Math.min(scaleX, scaleY, 1));
      setPosition({ x: 0, y: 0 });
    }
  }, [imageSize]);

  // 重置视图
  const resetView = useCallback(() => {
    setScale(1);
    setRotation(0);
    setPosition({ x: 0, y: 0 });
  }, []);

  // 自动适应窗口
  useEffect(() => {
    if (!isLoading && imageSize.width > 0) {
      fitToWindow();
    }
  }, [isLoading, imageSize, fitToWindow]);

  return (
    <div className="flex flex-col h-full">
      {/* 工具栏 */}
      <div className="flex flex-wrap items-center gap-2 px-3 py-1.5 border-b bg-forensics-panel text-[11px] shrink-0">
        <Image size={12} className="text-forensics-muted" />

        {/* 缩放控制 */}
        <Button
          type="button"
          variant="viewerControl"
          size="iconSm"
          onClick={() => setScale((s) => Math.min(10, s * 1.2))}
          title="放大"
          aria-label="放大"
        >
          <ZoomIn size={14} />
        </Button>
        <span className="min-w-[3.5rem] text-center font-mono text-forensics-muted">
          {Math.round(scale * 100)}%
        </span>
        <Button
          type="button"
          variant="viewerControl"
          size="iconSm"
          onClick={() => setScale((s) => Math.max(0.1, s * 0.8))}
          title="缩小"
          aria-label="缩小"
        >
          <ZoomOut size={14} />
        </Button>

        <div className="w-px h-4 bg-forensics-border-strong" />

        {/* 旋转 */}
        <Button
          type="button"
          variant="viewerControl"
          size="iconSm"
          onClick={() => setRotation((r) => (r + 90) % 360)}
          title="旋转"
          aria-label="旋转"
        >
          <RotateCw size={14} />
        </Button>

        {/* 适应窗口 */}
        <Button
          type="button"
          variant="viewerControl"
          size="iconSm"
          onClick={fitToWindow}
          title="适应窗口"
          aria-label="适应窗口"
        >
          <Maximize size={14} />
        </Button>

        {/* 重置 */}
        <Button
          type="button"
          variant="viewerControl"
          size="xs"
          onClick={resetView}
          className="h-6 text-[10px]"
          title="重置视图"
        >
          重置
        </Button>

        <div className="flex-1" />

        {/* 图片信息 */}
        {imageSize.width > 0 && (
          <span className="text-forensics-muted-lighter font-mono">
            {imageSize.width} × {imageSize.height}
          </span>
        )}

        {/* 下载按钮 */}
        {fileName && (
          <a
            href={src}
            download={fileName}
            className="p-1 hover:bg-forensics-hover rounded-none"
            title="下载"
          >
            <Download size={14} />
          </a>
        )}
      </div>

      {/* 图片容器 */}
      <div
        ref={containerRef}
        className={`flex-1 overflow-hidden bg-forensics-panel-strong ${
          isDragging ? 'cursor-grabbing' : 'cursor-grab'
        }`}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        {/* 加载状态 */}
        {isLoading && (
          <div className="flex items-center justify-center h-full text-forensics-muted-lighter">
            加载中...
          </div>
        )}

        {/* 错误状态 */}
        {error && (
          <div className="flex items-center justify-center h-full text-forensics-error-text">
            {error}
          </div>
        )}

        {/* 图片 */}
        <div
          className="w-full h-full flex items-center justify-center"
          style={{
            transform: `translate(${position.x}px, ${position.y}px) scale(${scale}) rotate(${rotation}deg)`,
            transition: isDragging ? 'none' : 'transform 0.1s ease-out',
          }}
        >
          <img
            ref={imgRef}
            src={src}
            alt={fileName || 'Preview'}
            onLoad={handleLoad}
            onError={handleError}
            className="max-w-full max-h-full object-contain select-none"
            draggable={false}
            style={{ display: isLoading ? 'none' : 'block' }}
          />
        </div>
      </div>

      {/* 状态栏 */}
      <div className="flex items-center gap-3 px-3 py-1 border-t bg-forensics-panel text-[10px] text-forensics-muted-lighter shrink-0">
        <span>{mimeType || '图片'}</span>
        {fileName && (
          <>
            <span className="text-forensics-muted-lighter">|</span>
            <span className="max-w-[50%] truncate">{fileName}</span>
          </>
        )}
      </div>
    </div>
  );
}
