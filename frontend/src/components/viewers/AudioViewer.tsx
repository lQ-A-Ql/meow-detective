/**
 * AudioViewer - 音频预览组件
 *
 * 功能：
 * - 播放/暂停
 * - 进度条拖拽
 * - 音量控制
 * - 音频信息显示
 */

import { useRef, useState, useEffect, useCallback } from 'react';
import { Play, Pause, Volume2, VolumeX, Music, SkipBack, SkipForward } from 'lucide-react';
import { Button } from '@/app/components/ui/button';

interface AudioViewerProps {
  /** 音频 URL */
  src: string;
  /** MIME 类型 */
  mimeType?: string;
  /** 文件名 */
  fileName?: string;
}

export function AudioViewer({ src, mimeType, fileName }: AudioViewerProps) {
  const audioRef = useRef<HTMLAudioElement>(null);

  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Reset display state when switching between preview sources.
  useEffect(() => {
    const audio = audioRef.current;
    audio?.pause();
    if (audio) {
      audio.currentTime = 0;
    }
    setIsPlaying(false);
    setCurrentTime(0);
    setDuration(0);
    setIsLoading(true);
    setError(null);
  }, [src]);

  // 播放/暂停
  const togglePlay = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) {
      return;
    }

    if (isPlaying) {
      audio.pause();
      return;
    }

    setError(null);
    audio.play().catch((e) => {
      setIsPlaying(false);
      setError(`播放失败: ${e.message}`);
    });
  }, [isPlaying]);

  // 快进/快退
  const skip = useCallback((seconds: number) => {
    if (audioRef.current) {
      audioRef.current.currentTime = Math.max(
        0,
        Math.min(duration, audioRef.current.currentTime + seconds)
      );
    }
  }, [duration]);

  // 切换静音
  const toggleMute = useCallback(() => {
    if (audioRef.current) {
      audioRef.current.muted = !isMuted;
      setIsMuted(!isMuted);
    }
  }, [isMuted]);

  // 音频事件监听
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const handleTimeUpdate = () => setCurrentTime(audio.currentTime);
    const handleLoadedMetadata = () => {
      setDuration(audio.duration);
      setIsLoading(false);
    };
    const handleEnded = () => setIsPlaying(false);
    const handlePlay = () => setIsPlaying(true);
    const handlePause = () => setIsPlaying(false);
    const handleError = () => {
      const mediaError = audio.error;
      let errorMsg = '音频加载失败';
      
      if (mediaError) {
        switch (mediaError.code) {
          case MediaError.MEDIA_ERR_ABORTED:
            errorMsg = '音频加载被中止';
            break;
          case MediaError.MEDIA_ERR_NETWORK:
            errorMsg = '网络错误，请检查文件路径';
            break;
          case MediaError.MEDIA_ERR_DECODE:
            errorMsg = '音频解码失败，格式可能不支持';
            break;
          case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
            errorMsg = '音频格式不支持或文件损坏';
            break;
        }
      }
      
      setError(errorMsg);
      setIsLoading(false);
    };

    audio.addEventListener('timeupdate', handleTimeUpdate);
    audio.addEventListener('loadedmetadata', handleLoadedMetadata);
    audio.addEventListener('ended', handleEnded);
    audio.addEventListener('play', handlePlay);
    audio.addEventListener('pause', handlePause);
    audio.addEventListener('error', handleError);

    return () => {
      audio.removeEventListener('timeupdate', handleTimeUpdate);
      audio.removeEventListener('loadedmetadata', handleLoadedMetadata);
      audio.removeEventListener('ended', handleEnded);
      audio.removeEventListener('play', handlePlay);
      audio.removeEventListener('pause', handlePause);
      audio.removeEventListener('error', handleError);
    };
  }, []);

  // 格式化时间
  const formatTime = (seconds: number) => {
    if (!isFinite(seconds)) return '0:00';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  // 计算进度百分比
  const progress = duration > 0 ? (currentTime / duration) * 100 : 0;

  return (
    <div className="flex flex-col h-full bg-forensics-850 text-white p-6">
      {/* 音频图标 */}
      <div className="flex items-center justify-center mb-6">
        <div className="w-24 h-24 rounded-none bg-forensics-800 flex items-center justify-center">
          <Music size={48} className="text-forensics-muted" />
        </div>
      </div>

      {/* 文件名 */}
      {fileName && (
        <div className="text-[14px] font-light mb-2 text-center truncate">
          {fileName}
        </div>
      )}

      {/* 加载/错误状态 */}
      {isLoading && (
        <div className="text-center text-forensics-muted-lighter text-[12px] mb-4">加载中...</div>
      )}
      {error && (
        <div className="text-center text-forensics-error-text text-[12px] mb-4">{error}</div>
      )}

      {/* 进度条 */}
      <div className="relative w-full mb-4 focus-within:ring-2 focus-within:ring-white/40">
        <div className="relative h-1.5 bg-forensics-text-secondary rounded-none overflow-hidden">
          <div
            className="absolute left-0 top-0 h-full bg-forensics-surface rounded-none transition-colors duration-500 duration-100"
            style={{ width: `${progress}%` }}
          />
        </div>
        <input
          type="range"
          min={0}
          max={duration || 0}
          step={0.1}
          value={currentTime}
          onChange={(e) => {
            const time = parseFloat(e.target.value);
            if (audioRef.current) {
              audioRef.current.currentTime = time;
            }
            setCurrentTime(time);
          }}
          aria-label="音频播放进度"
          className="absolute inset-x-0 -top-2 h-5 opacity-0 cursor-pointer"
        />
      </div>

      {/* 时间显示 */}
      <div className="flex justify-between text-[11px] text-forensics-muted-lighter mb-6 font-mono">
        <span>{formatTime(currentTime)}</span>
        <span>{formatTime(duration)}</span>
      </div>

      {/* 控制按钮 */}
      <div className="flex items-center justify-center gap-6">
        {/* 快退 */}
        <Button
          type="button"
          variant="mediaControl"
          size="mediaIcon"
          onClick={() => skip(-10)}
          aria-label="快退 10 秒"
        >
          <SkipBack size={20} />
        </Button>

        {/* 播放/暂停 */}
        <Button
          type="button"
          variant="mediaPrimaryControl"
          size="mediaPrimary"
          onClick={togglePlay}
          aria-label={isPlaying ? '暂停' : '播放'}
        >
          {isPlaying ? <Pause size={24} /> : <Play size={24} className="ml-1" />}
        </Button>

        {/* 快进 */}
        <Button
          type="button"
          variant="mediaControl"
          size="mediaIcon"
          onClick={() => skip(10)}
          aria-label="快进 10 秒"
        >
          <SkipForward size={20} />
        </Button>
      </div>

      {/* 音量控制 */}
      <div className="flex items-center justify-center gap-3 mt-6">
        <Button
          type="button"
          variant="mediaControl"
          size="iconSm"
          onClick={toggleMute}
          aria-label={isMuted ? '取消静音' : '静音'}
        >
          {isMuted ? <VolumeX size={16} /> : <Volume2 size={16} />}
        </Button>
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={isMuted ? 0 : volume}
          onChange={(e) => {
            const vol = parseFloat(e.target.value);
            setVolume(vol);
            setIsMuted(vol === 0);
            if (audioRef.current) {
              audioRef.current.volume = vol;
            }
          }}
          className="w-24 h-1 bg-forensics-text-secondary rounded-none appearance-none cursor-pointer"
        />
        <span className="text-[10px] text-forensics-muted w-8">
          {Math.round((isMuted ? 0 : volume) * 100)}%
        </span>
      </div>

      {/* 文件信息 */}
      <div className="mt-6 text-center text-[10px] text-forensics-muted">
        <span>{mimeType || 'audio'}</span>
      </div>

      {/* 隐藏的音频元素 */}
      <audio ref={audioRef} src={src} preload="metadata" />
    </div>
  );
}
