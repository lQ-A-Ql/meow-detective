import type { LocalSettings } from '@/lib/settings';

interface ImportPerformanceSectionProps {
  maxImportWorkers: string;
  maxAnalysisWorkers: string;
  importAnalysisMode: LocalSettings['importAnalysisMode'];
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function ImportPerformanceSection({
  maxImportWorkers,
  maxAnalysisWorkers,
  importAnalysisMode,
  setSettings,
}: ImportPerformanceSectionProps) {
  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-semibold text-[#333]">导入性能</span>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
          <span className="block text-[11px] font-semibold text-[#555]">枚举 Worker 上限</span>
          <input
            value={maxImportWorkers}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxImportWorkers: event.target.value,
              }))
            }
            inputMode="numeric"
            placeholder="自动"
            className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-[#999]">E01/RAW 分区枚举，空值使用自动。</span>
        </label>
        <label className="block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
          <span className="block text-[11px] font-semibold text-[#555]">分析 Worker 上限</span>
          <input
            value={maxAnalysisWorkers}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxAnalysisWorkers: event.target.value,
              }))
            }
            inputMode="numeric"
            placeholder="自动"
            className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-[#999]">artifact/timeline/text 分析池，空值使用逻辑核心。</span>
        </label>
      </div>
      <label className="mt-3 block border border-[#e0e0e0] bg-[#f8f8f8] p-3">
        <span className="block text-[11px] font-semibold text-[#555]">导入分析模式</span>
        <select
          value={importAnalysisMode}
          onChange={(event) =>
            setSettings((current) => ({
              ...current,
              importAnalysisMode:
                event.target.value === 'fullContent'
                  ? 'fullContent'
                  : event.target.value === 'budgetedContent'
                    ? 'budgetedContent'
                    : 'metadataOnly',
            }))
          }
          className="mt-2 w-full border border-[#ccc] bg-white px-2 py-1 text-[12px]"
        >
          <option value="metadataOnly">Metadata only (E01/RAW 推荐)</option>
          <option value="budgetedContent">Budgeted content (目录导入推荐)</option>
          <option value="fullContent">Full content (高内存风险)</option>
        </select>
        <span className="mt-1 block text-[10px] text-[#999]">
          E01/RAW 默认只做元数据与时间线；内容读取和全文索引需显式开启预算模式。
        </span>
      </label>
    </section>
  );
}
