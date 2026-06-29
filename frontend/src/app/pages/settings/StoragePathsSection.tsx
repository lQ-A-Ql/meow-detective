import { FolderOpen, HardDrive } from 'lucide-react';
import type { LocalSettings } from '@/lib/settings';

interface StoragePathsSectionProps {
  caseRoot: string;
  imageSearchPaths: string;
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function StoragePathsSection({ caseRoot, imageSearchPaths, setSettings }: StoragePathsSectionProps) {
  return (
    <>
      <section>
        <div className="flex items-center gap-2 mb-3">
          <FolderOpen size={14} className="text-[#888]" />
          <label htmlFor="settings-case-root" className="text-[13px] font-semibold text-[#333]">
            案件默认存储路径
          </label>
        </div>
        <input
          id="settings-case-root"
          value={caseRoot}
          onChange={(event) =>
            setSettings((current) => ({ ...current, caseRoot: event.target.value }))
          }
          className="w-full max-w-3xl bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#111]"
        />
        <div className="mt-1 text-[10px] text-[#999]">
          仅用于预填首页新建案件的父目录，实际存储位置以首页填写路径为准。
        </div>
      </section>

      <section>
        <div className="flex items-center gap-2 mb-3">
          <HardDrive size={14} className="text-[#888]" />
          <label htmlFor="settings-image-search-paths" className="text-[13px] font-semibold text-[#333]">
            镜像搜索路径
          </label>
        </div>
        <input
          id="settings-image-search-paths"
          value={imageSearchPaths}
          onChange={(event) =>
            setSettings((current) => ({ ...current, imageSearchPaths: event.target.value }))
          }
          className="w-full max-w-3xl bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#111]"
        />
        <div className="mt-1 text-[10px] text-[#999]">导入数据源时自动搜索的镜像目录（分号分隔）</div>
      </section>
    </>
  );
}
