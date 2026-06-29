import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import type { FileEntryRow, FileTreeNode } from '@/types/models';

interface FileBrowserInspectorProps {
  selectedFile?: FileEntryRow;
  activeDirectoryPath?: string;
  currentDirectory?: FileTreeNode;
  extractFile: {
    mutate: (file: FileEntryRow) => void;
    isPending: boolean;
  };
  onViewTimeline: () => void;
}

export function FileBrowserInspector({
  selectedFile,
  activeDirectoryPath,
  currentDirectory,
  extractFile,
  onViewTimeline,
}: FileBrowserInspectorProps) {
  return (
    <InspectorPane
      className="hidden lg:flex"
      title="对象检查器"
      subtitle={
        selectedFile
          ? `已选对象 ${selectedFile.name}`
          : '未选中文件对象'
      }
      widthClassName="w-80"
    >
      <div className="space-y-5">
        <InspectorSection title="对象标识">
          <InspectorValue
            value={selectedFile?.name ?? '-'}
            mono
            strong
          />
        </InspectorSection>

        <InspectorSection title="来源路径">
          <InspectorValue
            value={
              selectedFile?.path ??
              activeDirectoryPath ??
              currentDirectory?.name ??
              '-'
            }
            mono
          />
        </InspectorSection>

        <InspectorSection title="时间戳 (MACB)">
          <div className="font-mono text-[11px] grid grid-cols-[30px_1fr] gap-1">
            <div className="text-[#888]">M</div>
            <div className="text-[#333]">
              {selectedFile?.modifiedAt ?? '-'}
            </div>
            <div className="text-[#888]">A</div>
            <div className="text-[#333]">
              {selectedFile?.accessedAt ?? '-'}
            </div>
            <div className="text-[#888]">C</div>
            <div className="text-[#333]">
              {selectedFile?.changedAt ?? '-'}
            </div>
            <div className="text-[#888]">B</div>
            <div className="text-[#333]">
              {selectedFile?.createdAt ?? '-'}
            </div>
          </div>
        </InspectorSection>

        <InspectorSection title="摘要字段">
          <InspectorValue
            value={selectedFile?.hashSha256 ?? '-'}
            mono
          />
        </InspectorSection>

        <InspectorSection title="对象状态">
          <div className="font-mono text-[11px] grid grid-cols-[60px_1fr] gap-1">
            <div className="text-[#888]">deleted</div>
            <div className="text-[#333]">{selectedFile?.deleted ? 'true' : 'false'}</div>
            <div className="text-[#888]">hidden</div>
            <div className="text-[#333]">{selectedFile?.hidden ? 'true' : 'false'}</div>
            <div className="text-[#888]">system</div>
            <div className="text-[#333]">{selectedFile?.system ? 'true' : 'false'}</div>
          </div>
        </InspectorSection>

        <InspectorSection title="操作">
          <div className="flex flex-col gap-2">
            <button
              type="button"
              onClick={() => {
                if (selectedFile) {
                  extractFile.mutate(selectedFile);
                }
              }}
              disabled={!selectedFile || extractFile.isPending}
              className="w-full border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] py-1.5 text-center text-[11px] rounded-[2px] cursor-pointer font-medium disabled:opacity-50"
            >
              {extractFile.isPending ? '提取中...' : '提取文件'}
            </button>
            <button
              onClick={onViewTimeline}
              className="w-full border border-transparent text-[#666] hover:text-[#111] py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline"
            >
              在时间线中查看
            </button>
          </div>
        </InspectorSection>
      </div>
    </InspectorPane>
  );
}
