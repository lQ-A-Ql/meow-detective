/**
 * VideoViewer - 视频预览组件
 *
 * 功能：
 * - 播放/暂停/停止
 * - 进度条拖拽
 * - 音量控制
 * - 全屏播放
 */

import { useRef, useState, useEffect, useCallback } from 'react';
import {
  Play,
  Pause,
  Volume2,
  VolumeX,
  Maximize,
  Minimize,
  Video,
  SkipBack,
  SkipForward,
} from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Slider } from '@/app/components/ui/slider';
import { useTranslation } from 'react-i18next';

interface VideoViewerProps {
  /** 视频 URL */
  src: string;
  /** MIME 类型 */
  mimeType?: string;
  /** 文件名 */
  fileName?: string;
}

export function VideoViewer({ src, mimeType, fileName }: VideoViewerProps) {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Reset display state when switching between preview sources.
  useEffect(() => {
    const video = videoRef.current;
    video?.pause();
    if (video) {
      video.currentTime = 0;
    }
    setIsPlaying(false);
    setCurrentTime(0);
    setDuration(0);
    setIsLoading(true);
    setError(null);
  }, [src]);

  // 播放/暂停
  const togglePlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) {
      return;
    }

    if (isPlaying) {
      video.pause();
      return;
    }

    setError(null);
    video.play().catch((e) => {
      setIsPlaying(false);
      setError(t('media.errors.playback', { message: e.message }));
    });
  }, [isPlaying]);

  // 快进/快退
  const skip = useCallback((seconds: number) => {
    if (videoRef.current) {
      videoRef.current.currentTime = Math.max(
        0,
        Math.min(duration, videoRef.current.currentTime + seconds)
      );
    }
  }, [duration]);

  // 切换静音
  const toggleMute = useCallback(() => {
    if (videoRef.current) {
      videoRef.current.muted = !isMuted;
      setIsMuted(!isMuted);
    }
  }, [isMuted]);

  // 切换全屏
  const toggleFullscreen = useCallback(() => {
    if (containerRef.current) {
      if (!document.fullscreenElement) {
        containerRef.current.requestFullscreen().catch(() => {});
        setIsFullscreen(true);
      } else {
        document.exitFullscreen().catch(() => {});
        setIsFullscreen(false);
      }
    }
  }, []);

  // Keep fullscreen button state in sync with browser fullscreen changes.
  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(document.fullscreenElement === containerRef.current);
    };

    document.addEventListener('fullscreenchange', handleFullscreenChange);
    return () => document.removeEventListener('fullscreenchange', handleFullscreenChange);
  }, []);

  // 视频事件监听
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const handleTimeUpdate = () => setCurrentTime(video.currentTime);
    const handleLoadedMetadata = () => {
      setDuration(video.duration);
      setIsLoading(false);
    };
    const handleEnded = () => setIsPlaying(false);
    const handlePlay = () => setIsPlaying(true);
    const handlePause = () => setIsPlaying(false);
    const handleError = () => {
      const mediaError = video.error;
      let errorMsg = t('media.errors.videoLoad');
      
      if (mediaError) {
        switch (mediaError.code) {
          case MediaError.MEDIA_ERR_ABORTED:
            errorMsg = t('media.errors.aborted');
            break;
          case MediaError.MEDIA_ERR_NETWORK:
            errorMsg = t('media.errors.network');
            break;
          case MediaError.MEDIA_ERR_DECODE:
            errorMsg = t('media.errors.videoDecode');
            break;
          case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
            errorMsg = t('media.errors.videoUnsupported');
            break;
        }
      }
      
      setError(errorMsg);
      setIsLoading(false);
    };

    video.addEventListener('timeupdate', handleTimeUpdate);
    video.addEventListener('loadedmetadata', handleLoadedMetadata);
    video.addEventListener('ended', handleEnded);
    video.addEventListener('play', handlePlay);
    video.addEventListener('pause', handlePause);
    video.addEventListener('error', handleError);

    return () => {
      video.removeEventListener('timeupdate', handleTimeUpdate);
      video.removeEventListener('loadedmetadata', handleLoadedMetadata);
      video.removeEventListener('ended', handleEnded);
      video.removeEventListener('play', handlePlay);
      video.removeEventListener('pause', handlePause);
      video.removeEventListener('error', handleError);
    };
  }, []);

  // 格式化时间
  const formatTime = (seconds: number) => {
    if (!isFinite(seconds)) return '0:00';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div ref={containerRef} className="flex flex-col h-full bg-forensics-900">
      {/* 视频区域 */}
      <div className="flex-1 flex items-center justify-center relative">
        {isLoading && (
          <div className="absolute text-white text-[14px]">{t('media.loading')}</div>
        )}
        {error && (
          <div className="absolute text-forensics-error-text text-[14px]">{error}</div>
        )}
        <video
          ref={videoRef}
          src={src}
          className="max-w-full max-h-full"
          onClick={togglePlay}
          playsInline
        />
      </div>

      {/* 控制栏 */}
      <div className="flex flex-wrap items-center gap-3 px-4 py-2 bg-forensics-850 text-white text-[12px] shrink-0">
        {/* 播放/暂停 */}
        <Button type="button" variant="mediaControl" size="mediaIcon" onClick={togglePlay} aria-label={isPlaying ? t('media.controls.pause') : t('media.controls.play')}>
          {isPlaying ? <Pause size={18} /> : <Play size={18} />}
        </Button>

        {/* 快退 */}
        <Button type="button" variant="mediaControl" size="mediaIcon" onClick={() => skip(-10)} aria-label={t('media.controls.skipBack')}>
          <SkipBack size={16} />
        </Button>

        {/* 快进 */}
        <Button type="button" variant="mediaControl" size="mediaIcon" onClick={() => skip(10)} aria-label={t('media.controls.skipForward')}>
          <SkipForward size={16} />
        </Button>

        {/* 进度条 */}
        <Slider
          min={0}
          max={duration || 0}
          step={0.1}
          value={[currentTime]}
          onValueChange={([time]) => {
            if (videoRef.current) videoRef.current.currentTime = time;
            setCurrentTime(time);
          }}
          aria-label={t('media.videoProgress')}
          className="min-w-24 flex-1"
        />

        {/* 时间 */}
        <span className="w-24 text-center font-mono text-[11px]">
          {formatTime(currentTime)} / {formatTime(duration)}
        </span>

        {/* 音量 */}
        <Button type="button" variant="mediaControl" size="mediaIcon" onClick={toggleMute} aria-label={isMuted ? t('media.controls.unmute') : t('media.controls.mute')}>
          {isMuted ? <VolumeX size={16} /> : <Volume2 size={16} />}
        </Button>
        <Slider
          min={0}
          max={1}
          step={0.05}
          value={[isMuted ? 0 : volume]}
          onValueChange={([vol]) => {
            setVolume(vol);
            setIsMuted(vol === 0);
            if (videoRef.current) videoRef.current.volume = vol;
          }}
          aria-label={t('media.volume')}
          className="w-20"
        />

        {/* 全屏 */}
        <Button type="button" variant="mediaControl" size="mediaIcon" onClick={toggleFullscreen} aria-label={isFullscreen ? t('media.controls.exitFullscreen') : t('media.controls.fullscreen')}>
          {isFullscreen ? <Minimize size={16} /> : <Maximize size={16} />}
        </Button>
      </div>

      {/* 状态栏 */}
      <div className="flex items-center gap-3 px-3 py-1 bg-forensics-text text-[10px] text-forensics-muted shrink-0">
        <Video size={10} />
        <span>{mimeType || t('media.video')}</span>
        {fileName && (
          <>
            <span className="text-forensics-text-secondary">|</span>
            <span className="max-w-[50%] truncate">{fileName}</span>
          </>
        )}
      </div>
    </div>
  );
}
