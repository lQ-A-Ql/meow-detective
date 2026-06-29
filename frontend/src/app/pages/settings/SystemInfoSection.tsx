export function SystemInfoSection() {
  return (
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
        <div className="flex justify-between border-b border-[#eee] pb-1">
          <span>MCP 协议</span>
          <span>v1.0 (Resources/Tools/Prompts)</span>
        </div>
      </div>
    </section>
  );
}
