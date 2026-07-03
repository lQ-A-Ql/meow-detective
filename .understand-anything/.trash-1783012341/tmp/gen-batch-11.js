const fs = require('fs');

const extract = JSON.parse(fs.readFileSync("D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-11.json", "utf8"));

function N(id, type, name, filePath, summary, tags, complexity, extra) {
  const n = { id, type, name, filePath, summary, tags, complexity };
  if (extra) Object.assign(n, extra);
  return n;
}

function E(source, target, type, weight) {
  return { source, target, type, direction: "forward", weight };
}

const nodes = [];
const edges = [];
const nodeIds = new Set();

function addNode(n) { if (!nodeIds.has(n.id)) { nodeIds.add(n.id); nodes.push(n); } }
function addEdge(e) { if (e.source === e.target) return; edges.push(e); }

const bid = {
  "frontend/src/app/pages/CaseActions.test.tsx": ["frontend/src/app/pages/CaseActions.tsx"],
  "frontend/src/app/pages/CaseActions.tsx": ["frontend/src/types/models.ts"],
  "frontend/src/app/pages/CaseHome.test.tsx": ["frontend/src/app/pages/CaseHome.tsx"],
  "frontend/src/app/pages/CaseHome.tsx": ["frontend/src/app/pages/CaseActions.tsx","frontend/src/app/pages/CaseOverview.tsx","frontend/src/features/case/hooks.ts","frontend/src/features/files/hooks.ts","frontend/src/features/jobs/hooks.ts","frontend/src/lib/api/files.ts","frontend/src/lib/api/settings.ts","frontend/src/lib/settings.ts","frontend/src/types/models.ts"],
  "frontend/src/app/pages/CaseOverview.tsx": ["frontend/src/components/status/InlineProgressRow.tsx","frontend/src/lib/partition-display.ts","frontend/src/types/models.ts"],
  "frontend/src/app/pages/Reports.test.tsx": ["frontend/src/app/pages/Reports.tsx"],
  "frontend/src/app/pages/Reports.tsx": ["frontend/src/features/case/hooks.ts","frontend/src/features/jobs/import-event-state.ts","frontend/src/features/reports/hooks.ts","frontend/src/lib/api/reports.ts","frontend/src/types/models.ts"],
  "frontend/src/app/pages/V3Dashboard.test.tsx": ["frontend/src/app/pages/V3Dashboard.tsx"],
  "frontend/src/app/pages/V3Dashboard.tsx": ["frontend/src/app/components/ui/button.tsx","frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/components/analysis/AnalysisPanels.tsx","frontend/src/components/dashboard/ArtifactStatsSection.tsx","frontend/src/components/dashboard/BatchStatusSection.tsx","frontend/src/components/dashboard/CorrelationStatsSection.tsx","frontend/src/components/dashboard/DataSourceCoverageSection.tsx","frontend/src/components/dashboard/GraphStatsSection.tsx","frontend/src/components/dashboard/PlatformCoverageSection.tsx","frontend/src/components/dashboard/RulePackStatusSection.tsx","frontend/src/components/dashboard/TimelineOverviewSection.tsx","frontend/src/features/analysis/hooks.ts","frontend/src/features/artifacts/hooks.ts","frontend/src/features/case/hooks.ts","frontend/src/features/graph/hooks.ts","frontend/src/features/timeline/hooks.ts"],
  "frontend/src/app/pages/V3ScoreCards.tsx": ["frontend/src/lib/api/client.ts"],
  "frontend/src/app/pages/file-tree-utils.ts": ["frontend/src/types/models.ts"],
  "frontend/src/app/pages/use-file-browser.ts": ["frontend/src/features/case/hooks.ts","frontend/src/features/files/hooks.ts","frontend/src/features/files/hooks/use-file-pagination.ts","frontend/src/features/files/hooks/use-file-preview.ts","frontend/src/features/files/hooks/use-file-selection.ts","frontend/src/features/files/hooks/use-file-tree.ts","frontend/src/stores/ui-store.ts","frontend/src/types/models.ts"],
  "frontend/src/app/providers.tsx": ["frontend/src/features/cache-invalidation.ts","frontend/src/i18n/index.ts","frontend/src/lib/events/subscribers.ts","frontend/src/types/models.ts"],
  "frontend/src/components/analysis/CorrelationPanel.tsx": ["frontend/src/components/analysis/V2GovernancePanels.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/analysis/LimitationsPanel.tsx": ["frontend/src/types/models.ts"],
  "frontend/src/components/analysis/V2GovernancePanels.tsx": ["frontend/src/types/models.ts"],
  "frontend/src/components/analysis/VerificationPanel.tsx": ["frontend/src/components/analysis/V2GovernancePanels.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/ArtifactStatsSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/BatchStatusSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/CorrelationStatsSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/DataSourceCoverageSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/GraphStatsSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/components/graph/GraphVisualizationSection.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/PlatformCoverageSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/RulePackStatusSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/dashboard/TimelineOverviewSection.tsx": ["frontend/src/app/pages/V3ScoreCards.tsx"],
  "frontend/src/components/gql/GqlEditor.test.tsx": ["frontend/src/components/gql/GqlEditor.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/gql/GqlEditor.tsx": ["frontend/src/components/gql/GqlQueryInput.tsx","frontend/src/components/gql/GqlResultView.tsx","frontend/src/types/models.ts"],
  "frontend/src/components/gql/GqlResultView.test.tsx": ["frontend/src/components/gql/GqlResultView.tsx","frontend/src/types/models.ts"]
};

const fileInfo = {};
fileInfo["frontend/src/app/pages/CaseActions.test.tsx"] = {summary:"CaseActions 组件的单元测试，覆盖案例创建、打开和导入的交互流程，包含表单状态管理和异步操作模拟。",tags:["test","unit-test","case-management","frontend"],complexity:"complex"};
fileInfo["frontend/src/app/pages/CaseActions.tsx"] = {summary:"案例操作面板组件，包含 CaseWelcomeForms（创建/打开案例）和 ImportSection（证据导入）两个子模块，是案例管理入口的核心 UI。",tags:["component","case-management","entry-point","ui"],complexity:"complex"};
fileInfo["frontend/src/app/pages/CaseHome.test.tsx"] = {summary:"CaseHome 页面的综合单元测试，验证案例首页的数据源展示、作业状态、指标面板和重命名/删除交互。",tags:["test","unit-test","case-management","frontend"],complexity:"complex"};
fileInfo["frontend/src/app/pages/CaseHome.tsx"] = {summary:"案例首页主组件，集成案例操作、概览指标、数据源管理和导入对话框，是进入案件后的核心枢纽页面。",tags:["page","entry-point","case-management","ui"],complexity:"complex"};
fileInfo["frontend/src/app/pages/CaseOverview.tsx"] = {summary:"案例概览子面板集合，展示案例指标卡片、数据源列表、最近任务和最近对象，支撑 CaseHome 页面的信息呈现。",tags:["component","dashboard","case-management","ui"],complexity:"complex"};
fileInfo["frontend/src/app/pages/Reports.test.tsx"] = {summary:"Reports 报告页面的单元测试，验证报告模板展示和导出功能的基本渲染。",tags:["test","unit-test","reports","frontend"],complexity:"moderate"};
fileInfo["frontend/src/app/pages/Reports.tsx"] = {summary:"报告生成与导出页面，支持选择报告模板、格式（HTML/CSV/JSON）和数据范围，查看导出历史记录。",tags:["page","reports","export","ui"],complexity:"moderate"};
fileInfo["frontend/src/app/pages/V3Dashboard.test.tsx"] = {summary:"V3Dashboard 的综合单元测试，模拟 IntersectionObserver 和 API 响应以验证仪表盘的统计卡片渲染。",tags:["test","unit-test","dashboard","governance"],complexity:"complex"};
fileInfo["frontend/src/app/pages/V3Dashboard.tsx"] = {summary:"V3 治理仪表盘页面，聚合展示图谱统计、时间线、关联分析、治理评分等多个分析维度的实时数据面板。",tags:["page","dashboard","governance","analysis"],complexity:"moderate"};
fileInfo["frontend/src/app/pages/V3ScoreCards.tsx"] = {summary:"V3 仪表盘的通用展示组件库，提供 StatCard、SectionHeader、EmptyPlaceholder 和 errorMessage 工具函数，供各统计面板复用。",tags:["component","utility","dashboard","ui"],complexity:"simple"};
fileInfo["frontend/src/app/pages/file-tree-utils.ts"] = {summary:"文件树工具函数，提供树节点比较（sameTreeNode/sameTreeNodeList）和分页合并（mergeTreeNodePages）的逻辑。",tags:["utility","file-tree","data-processing"],complexity:"moderate"};
fileInfo["frontend/src/app/pages/use-file-browser.ts"] = {summary:"文件浏览器核心 React Hook，编排文件树、分页、预览、选择、跳转等子功能，管理 UI 状态与数据源的协调。",tags:["hook","file-browser","state-management","orchestration"],complexity:"complex"};
fileInfo["frontend/src/app/providers.tsx"] = {summary:"应用全局 Provider 组件，配置 React Query 客户端、订阅事件驱动的投影失效逻辑，包装 i18n 翻译支持。",tags:["provider","react-query","events","i18n"],complexity:"moderate"};
fileInfo["frontend/src/components/analysis/CorrelationPanel.tsx"] = {summary:"关联分析发布评分面板，展示关联覆盖度、信号置信度、运行状态和规则族评分，供治理仪表盘使用。",tags:["component","analysis","correlation","governance"],complexity:"moderate"};
fileInfo["frontend/src/components/analysis/LimitationsPanel.tsx"] = {summary:"已知限制面板，展示分析验证链中的已知局限性（影响链路、状态标记），辅助治理审计。",tags:["component","analysis","governance","limitations"],complexity:"moderate"};
fileInfo["frontend/src/components/analysis/V2GovernancePanels.tsx"] = {summary:"V2 治理核心面板集合，包含安全审计、事实源、运行时结果、发布门控和治理概览等子面板，是治理模块的可视化主干。",tags:["component","governance","analysis","audit","barrel"],complexity:"complex"};
fileInfo["frontend/src/components/analysis/VerificationPanel.tsx"] = {summary:"验证分析面板集合，展示验证链路评分、基准测试覆盖、支持矩阵和成熟度标记，支持治理质量评估。",tags:["component","analysis","verification","governance"],complexity:"complex"};
fileInfo["frontend/src/components/dashboard/ArtifactStatsSection.tsx"] = {summary:"仪表盘-工件统计区域，按工件族展示各数据源的工件数量分布。",tags:["component","dashboard","artifacts","statistics"],complexity:"simple"};
fileInfo["frontend/src/components/dashboard/BatchStatusSection.tsx"] = {summary:"仪表盘-批处理状态区域，展示批量作业的运行状态概览。",tags:["component","dashboard","batch","jobs"],complexity:"simple"};
fileInfo["frontend/src/components/dashboard/CorrelationStatsSection.tsx"] = {summary:"仪表盘-关联统计区域，展示关联分析中各规则族的覆盖度和线索数量分布。",tags:["component","dashboard","correlation","statistics"],complexity:"moderate"};
fileInfo["frontend/src/components/dashboard/DataSourceCoverageSection.tsx"] = {summary:"仪表盘-数据源覆盖区域，统计数据源类型与哈希校验覆盖状态。",tags:["component","dashboard","datasource","coverage"],complexity:"simple"};
fileInfo["frontend/src/components/dashboard/GraphStatsSection.tsx"] = {summary:"仪表盘-图谱统计区域，展示知识图谱的节点/边类型分布计数。",tags:["component","dashboard","graph","statistics"],complexity:"moderate"};
fileInfo["frontend/src/components/dashboard/PlatformCoverageSection.tsx"] = {summary:"仪表盘-平台覆盖区域，按 Windows/Linux/macOS/跨平台分类展示工件族支持情况。",tags:["component","dashboard","platform","coverage"],complexity:"moderate"};
fileInfo["frontend/src/components/dashboard/RulePackStatusSection.tsx"] = {summary:"仪表盘-规则包状态区域，展示已加载关联规则包的清单和启用状态。",tags:["component","dashboard","rules","configuration"],complexity:"simple"};
fileInfo["frontend/src/components/dashboard/TimelineOverviewSection.tsx"] = {summary:"仪表盘-时间线概览区域，展示已索引的时间线事件总数和加载状态。",tags:["component","dashboard","timeline","overview"],complexity:"simple"};
fileInfo["frontend/src/components/gql/GqlEditor.test.tsx"] = {summary:"GqlEditor 组件的单元测试，验证 GraphQL 查询编辑器的基本渲染和行为。",tags:["test","unit-test","graphql","editor"],complexity:"simple"};
fileInfo["frontend/src/components/gql/GqlEditor.tsx"] = {summary:"GraphQL 查询编辑器组件，集成查询输入、执行按钮和结果展示，用于知识图谱的交互式查询。",tags:["component","graphql","editor","query"],complexity:"simple"};
fileInfo["frontend/src/components/gql/GqlResultView.test.tsx"] = {summary:"GqlResultView 组件的单元测试，验证 GraphQL 查询结果的 JSON 渲染和错误状态显示。",tags:["test","unit-test","graphql","result-view"],complexity:"moderate"};

// Process each file
for (const r of extract.results) {
  const p = r.path;
  const fi = fileInfo[p] || {summary:"Unknown", tags:["unknown"], complexity:"simple"};
  const fileId = "file:" + p;

  addNode(N(fileId, "file", p.split("/").pop(), p, fi.summary, fi.tags, fi.complexity));

  // Function nodes - significance filter: 10+ lines OR exported
  for (const f of (r.functions || [])) {
    const lineCount = f.endLine - f.startLine;
    const isExported = (r.exports || []).some(function(e) { return e.name === f.name; });
    if (lineCount >= 10 || isExported) {
      const fid = "function:" + p + ":" + f.name;
      const isTestFile = p.indexOf(".test.") >= 0;
      const startLower = f.name.charAt(0) === f.name.charAt(0).toLowerCase() && f.name.charAt(0) !== f.name.charAt(0).toUpperCase();
      const isHelper = startLower && !isExported && isTestFile;
      const funcTags = isTestFile
        ? ["test-helper", "unit-test"]
        : isExported
          ? ["component", "exported"]
          : ["utility", "internal"];
      const funcSummary = isTestFile
        ? "测试辅助函数，用于设置 Vitest mock 和渲染测试页面。"
        : isExported
          ? "导出的 React 组件，负责相关 UI 渲染和交互逻辑。"
          : "内部辅助函数，封装相关逻辑计算。";

      const compl = lineCount > 30 ? "complex" : lineCount > 15 ? "moderate" : "simple";
      addNode(N(fid, "function", f.name, p, funcSummary, funcTags, compl, {lineRange: [f.startLine, f.endLine]}));

      // contains edge
      addEdge(E(fileId, fid, "contains", 1.0));

      // exports edge
      if (isExported) {
        addEdge(E(fileId, fid, "exports", 0.8));
      }
    }
  }

  // Import edges (1:1 rule)
  const imports = bid[p] || [];
  for (let i = 0; i < imports.length; i++) {
    addEdge(E(fileId, "file:" + imports[i], "imports", 0.7));
  }

  // tested_by edges: production -> test direction
  if (p.indexOf(".test.") >= 0) {
    const prodImport = imports.length > 0 ? imports[0] : null;
    if (prodImport) {
      addEdge(E("file:" + prodImport, fileId, "tested_by", 0.5));
    }
  }
}

// Additional non-import edges for within-batch relationships

// V2GovernancePanels re-exports from sibling analysis components
var v2Path = "frontend/src/components/analysis/V2GovernancePanels.tsx";
addEdge(E("file:" + v2Path, "file:frontend/src/components/analysis/VerificationPanel.tsx", "depends_on", 0.6));
addEdge(E("file:" + v2Path, "file:frontend/src/components/analysis/LimitationsPanel.tsx", "depends_on", 0.6));
addEdge(E("file:" + v2Path, "file:frontend/src/components/analysis/CorrelationPanel.tsx", "depends_on", 0.6));

// Dashboard sections depend on V3ScoreCards for shared StatCard/SectionHeader components
var dashSections = [
  "frontend/src/components/dashboard/ArtifactStatsSection.tsx",
  "frontend/src/components/dashboard/BatchStatusSection.tsx",
  "frontend/src/components/dashboard/CorrelationStatsSection.tsx",
  "frontend/src/components/dashboard/DataSourceCoverageSection.tsx",
  "frontend/src/components/dashboard/GraphStatsSection.tsx",
  "frontend/src/components/dashboard/PlatformCoverageSection.tsx",
  "frontend/src/components/dashboard/RulePackStatusSection.tsx",
  "frontend/src/components/dashboard/TimelineOverviewSection.tsx"
];
for (var di = 0; di < dashSections.length; di++) {
  addEdge(E("file:" + dashSections[di], "file:frontend/src/app/pages/V3ScoreCards.tsx", "depends_on", 0.6));
}

// V3Dashboard depends on its child dashboard sections (already captured via imports)

// CorrelationPanel depends on V2GovernancePanels (batched together)
// VerificationPanel depends on V2GovernancePanels (batched together)
// Both already captured via import edges

// GqlEditor calls GqlResultView and GqlQueryInput - already in imports

console.log("Total nodes: " + nodes.length);
console.log("Total edges: " + edges.length);

// Verify import edges count
var importEdgeCount = 0;
for (var ei = 0; ei < edges.length; ei++) { if (edges[ei].type === "imports") importEdgeCount++; }
var expectedImports = 0;
for (var kk in bid) { if (bid.hasOwnProperty(kk)) { expectedImports += bid[kk].length; } }
console.log("Import edges: " + importEdgeCount + " (expected: " + expectedImports + ")");

var parts = Math.ceil(Math.max(nodes.length / 60, edges.length / 120));
console.log("Parts needed: " + parts);

// Sort files alphabetically
var allFiles = extract.results.map(function(rr) { return rr.path; }).sort();

function filePathForNode(n) {
  if (n.filePath) return n.filePath;
  // For function/class nodes, derive from id
  var m = n.id.match(/^(function|class):(.+?):/);
  return m ? m[2] : null;
}

// Chunk files into parts
var chunkSize = Math.ceil(allFiles.length / parts);
var chunks = [];
for (var ci = 0; ci < allFiles.length; ci += chunkSize) {
  chunks.push(allFiles.slice(ci, ci + chunkSize));
}

for (var kk = 0; kk < chunks.length; kk++) {
  var partFilesSet = {};
  for (var fi = 0; fi < chunks[kk].length; fi++) { partFilesSet[chunks[kk][fi]] = true; }

  var partNodes = [];
  for (var ni = 0; ni < nodes.length; ni++) {
    var fp = filePathForNode(nodes[ni]);
    if (fp && partFilesSet[fp]) partNodes.push(nodes[ni]);
  }

  var partNodeIdSet = {};
  for (var pni = 0; pni < partNodes.length; pni++) { partNodeIdSet[partNodes[pni].id] = true; }

  // Include all edges (not just source-matching) since targets may be in same batch
  var partEdges = [];
  for (var ei2 = 0; ei2 < edges.length; ei2++) {
    var srcInPart = !!partNodeIdSet[edges[ei2].source];
    if (srcInPart) { partEdges.push(edges[ei2]); }
  }

  var suffix = parts === 1 ? "" : "-part-" + (kk + 1);
  var outPath = "D:/process/forensic/.understand-anything/intermediate/batch-11" + suffix + ".json";
  fs.writeFileSync(outPath, JSON.stringify({nodes: partNodes, edges: partEdges}, null, 2), "utf8");
  console.log("Written " + outPath + ": " + partNodes.length + " nodes, " + partEdges.length + " edges");
}

console.log("Done.");
