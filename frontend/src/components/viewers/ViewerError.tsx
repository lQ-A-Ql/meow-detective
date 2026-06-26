import { memo, useCallback } from 'react';
import { AlertCircle, Copy, RefreshCw } from 'lucide-react';
import type { ApiErrorDto } from '@/types/models';

interface ViewerErrorProps {
  /** The error object from the Tauri command */
  error: ApiErrorDto;
  /** Callback to retry the failed operation */
  onRetry?: () => void;
}

function copyErrorToClipboard(error: ApiErrorDto) {
  const lines = [
    `Code: ${error.code}`,
    `Message: ${error.message}`,
    `Category: ${error.category ?? 'unknown'}`,
  ];
  if (error.suggestion) lines.push(`Suggestion: ${error.suggestion}`);
  void navigator.clipboard.writeText(lines.join('\n'));
}

export const ViewerError = memo(function ViewerError({ error, onRetry }: ViewerErrorProps) {
  const handleCopy = useCallback(() => copyErrorToClipboard(error), [error]);

  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-white p-8">
      <div className="max-w-lg text-center">
        <AlertCircle size={36} className="mx-auto mb-3 text-[#e74c3c]" />

        <h3 className="mb-2 font-mono text-[13px] font-semibold text-[#111]">
          [{error.code}] 文件预览失败
        </h3>

        <p className="mb-4 text-[13px] leading-relaxed text-[#666]">
          {error.message}
        </p>

        {error.suggestion && (
          <div className="mb-4 rounded border border-[#f0d478] bg-[#fef9e7] px-4 py-3 text-left">
            <p className="text-[12px] leading-relaxed text-[#8a6d3b]">
              💡 {error.suggestion}
            </p>
          </div>
        )}

        <div className="flex items-center justify-center gap-2">
          <button
            onClick={handleCopy}
            className="inline-flex items-center gap-1.5 rounded border border-[#ddd] px-3 py-1.5 text-[12px] text-[#555] hover:bg-[#f5f5f5]"
            aria-label="复制错误详情"
          >
            <Copy size={14} />
            复制详情
          </button>
          {error.recoverable && onRetry && (
            <button
              onClick={onRetry}
              className="inline-flex items-center gap-1.5 rounded border border-[#3498db] bg-[#3498db] px-3 py-1.5 text-[12px] text-white hover:bg-[#2980b9]"
              aria-label="重试预览"
            >
              <RefreshCw size={14} />
              重试
            </button>
          )}
        </div>
      </div>
    </div>
  );
});
