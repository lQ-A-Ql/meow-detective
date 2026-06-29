import type { LocalSettings } from '@/lib/settings';

interface PreviewSectionProps {
  hexChunkBytes: string;
  maxViewerRangeLength: string;
  maxInlineImagePreviewBytes: string;
  maxInlineMediaPreviewBytes: string;
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function PreviewSection({
  hexChunkBytes,
  maxViewerRangeLength,
  maxInlineImagePreviewBytes,
  maxInlineMediaPreviewBytes,
  setSettings,
}: PreviewSectionProps) {
  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-semibold text-[#333]">预览</span>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
          <span className="block text-[11px] font-semibold text-[#555]">Hex 分块大小（字节）</span>
          <input
            value={hexChunkBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                hexChunkBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-[#999]">十六进制查看器每次读取字节数。</span>
        </label>
        <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
          <span className="block text-[11px] font-semibold text-[#555]">单请求最大范围（字节）</span>
          <input
            value={maxViewerRangeLength}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxViewerRangeLength: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-[#999]">文件预览单次请求返回上限。</span>
        </label>
        <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
          <span className="block text-[11px] font-semibold text-[#555]">内联图片预览上限（字节）</span>
          <input
            value={maxInlineImagePreviewBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxInlineImagePreviewBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-[#999]">超过此大小的图片将不直接内联显示。</span>
        </label>
        <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
          <span className="block text-[11px] font-semibold text-[#555]">内联媒体预览上限（字节）</span>
          <input
            value={maxInlineMediaPreviewBytes}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxInlineMediaPreviewBytes: event.target.value,
              }))
            }
            inputMode="numeric"
            className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-[#999]">超过此大小的媒体将不直接内联显示。</span>
        </label>
      </div>
    </section>
  );
}
