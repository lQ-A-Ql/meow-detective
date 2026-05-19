import { HardDrive, FolderOpen } from 'lucide-react';

export function Settings() {
  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="font-serif text-xl text-[#111] tracking-tight">设置</div>
        <div className="text-[#666] text-[11px] font-mono mt-1">应用配置与数据目录</div>
      </div>

      <div className="p-6 space-y-8">
        <section>
          <div className="flex items-center gap-2 mb-3">
            <FolderOpen size={14} className="text-[#888]" />
            <span className="text-[13px] font-semibold text-[#333]">案件目录</span>
          </div>
          <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#555]">
            C:\Cases
          </div>
          <div className="mt-1 text-[10px] text-[#999]">所有案件数据将存储在此目录下</div>
        </section>

        <section>
          <div className="flex items-center gap-2 mb-3">
            <HardDrive size={14} className="text-[#888]" />
            <span className="text-[13px] font-semibold text-[#333]">镜像搜索路径</span>
          </div>
          <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#555]">
            E:\cases\; D:\images\
          </div>
          <div className="mt-1 text-[10px] text-[#999]">导入数据源时自动搜索的镜像目录（分号分隔）</div>
        </section>

        <section>
          <div className="flex items-center gap-2 mb-3">
            <span className="text-[13px] font-semibold text-[#333]">系统信息</span>
          </div>
          <div className="space-y-2 text-[12px] font-mono text-[#666]">
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>版本</span>
              <span>Forensics Workbench 0.1.0</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>平台</span>
              <span>{navigator.platform || 'Windows'}</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>数据库</span>
              <span>SQLite (每个案件独立)</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>搜索引擎</span>
              <span>Tantivy (全文索引)</span>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
