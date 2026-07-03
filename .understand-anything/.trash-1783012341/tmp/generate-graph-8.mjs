// Generate knowledge graph for batch 8

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';

const PROJECT_ROOT = 'D:/process/forensic';
const TMP_DIR = join(PROJECT_ROOT, '.understand-anything', 'tmp');
const OUT_DIR = join(PROJECT_ROOT, '.understand-anything', 'intermediate');

// Read extraction results
const extractResults = JSON.parse(
  readFileSync(join(TMP_DIR, 'ua-file-extract-results-8.json'), 'utf8')
);

// Read batch data
const batches = JSON.parse(
  readFileSync(join(OUT_DIR, 'batches.json'), 'utf8')
);
const batch8 = batches.batches.find(b => b.batchIndex === 8);

const batchImportData = batch8.batchImportData;
const neighborMap = batch8.neighborMap;

const results = extractResults.results;

// ========== BUILD NODES AND EDGES ==========
const nodes = [];
const edges = [];

// Helper functions
function fmtLines(f) {
  // Lines less than 10 but exported -> include if >= 5 lines
  // Lines >= 10 -> include
  // Exported always include (but skip trivial < 3)
  return true; // Will filter later
}

const allFiles = results.map(r => r.path).sort();

// File nodes
for (const r of results) {
  const path = r.path;
  const name = path.split('/').pop();
  let complexity = 'simple';
  if (r.nonEmptyLines > 200) complexity = 'complex';
  else if (r.nonEmptyLines > 50) complexity = 'moderate';

  let summary = '';
  let tags = [];

  // Determine tags and summary based on path and content
  const isTest = path.includes('.test.') || path.includes('.spec.');

  if (path === 'frontend/src/app/components/ui/tooltip.tsx') {
    summary = '基于 Radix UI 的 Tooltip 组件封装，提供 TooltipProvider / Tooltip / TooltipTrigger / TooltipContent 四个子组件，支持延迟显示和自定义样式。';
    tags = ['component', 'ui', 'tooltip', 'radix-ui'];
  } else if (path === 'frontend/src/app/components/ui/use-mobile.ts') {
    summary = '响应式设计辅助 Hook，通过 matchMedia 监听视口宽度变化，返回当前是否为移动端（<768px）的布尔值。';
    tags = ['hook', 'ui', 'responsive', 'mobile'];
  } else if (path === 'frontend/src/app/components/ui/utils.ts') {
    summary = 'Tailwind CSS 类名合并工具函数，使用 clsx 和 tailwind-merge 实现条件类名拼接与冲突去重。';
    tags = ['utility', 'tailwind', 'css', 'styling'];
  } else if (path === 'frontend/src/app/pages/DataAnalysis.test.tsx') {
    summary = 'DataAnalysis 页面的集成测试套件，涵盖数据源选择、分析分类扫描、提取进度面板和摘要生成的渲染验证。';
    tags = ['test', 'integration-test', 'analysis', 'page'];
  } else if (path === 'frontend/src/app/pages/DataAnalysis.tsx') {
    summary = '数据分析主页面组件，整合系统信息、证据分类、注册表提取、浏览器历史、邮件、EVTX 事件和 Linux 痕迹等分析面板，支持按分类运行提取和 AI 摘要生成。';
    tags = ['page', 'analysis', 'entry-point', 'dashboard'];
  } else if (path === 'frontend/src/app/pages/V2Workbench.test.tsx') {
    summary = 'V2 工作台的集成测试套件，验证 V2 治理快照和关联快照的查询、刷新及 Demo 案例加载流程。';
    tags = ['test', 'integration-test', 'v2', 'workbench'];
  } else if (path === 'frontend/src/app/pages/V2Workbench.tsx') {
    summary = 'V2 分析工作台页面，提供治理快照面板和关联分析工作区的组合视图，支持 Demo 案例快捷加载。';
    tags = ['page', 'analysis', 'v2', 'workbench'];
  } else if (path === 'frontend/src/app/routes.test.ts') {
    summary = '路由配置的单元测试，验证 appRoutes 定义和 router 实例的正确创建。';
    tags = ['test', 'unit-test', 'routing'];
  } else if (path === 'frontend/src/app/routes.tsx') {
    summary = '应用路由配置模块，定义所有页面路由映射（懒加载），创建并导出 React Router 实例供全局使用。';
    tags = ['routing', 'entry-point', 'configuration', 'react-router'];
  } else if (path === 'frontend/src/components/analysis/AnalysisPanels.tsx') {
    summary = '分析面板模块的桶文件（barrel），集中重导出所有分析面板组件、状态指示器和工具函数，简化其他模块的导入路径。';
    tags = ['barrel', 're-export', 'analysis', 'index'];
  } else if (path === 'frontend/src/components/analysis/ClusterView.tsx') {
    summary = '关联簇卡片组件，展示单个关联簇的置信度、节点数、边数、时间线事件数以及涉及的文件家族标签，支持跳转。';
    tags = ['component', 'correlation', 'cluster', 'visualization'];
  } else if (path === 'frontend/src/components/analysis/CorrelationWorkspace.test.tsx') {
    summary = '关联分析工作区的集成测试，覆盖快照数据结构渲染、簇视图、线索筛选及跳转功能验证。';
    tags = ['test', 'integration-test', 'correlation', 'workspace'];
  } else if (path === 'frontend/src/components/analysis/CorrelationWorkspace.tsx') {
    summary = '关联分析主工作区组件，实现多维度关联线索仪表板：线索搜索筛选、置信度分级、簇关系图、文件跳转导航及摘要统计面板。';
    tags = ['component', 'correlation', 'workspace', 'analytics'];
  } else if (path === 'frontend/src/components/analysis/LeadDetail.test.tsx') {
    summary = '关联线索详情面板的单元测试，覆盖线索数据结构构造与节点展示验证。';
    tags = ['test', 'unit-test', 'lead', 'detail'];
  } else if (path === 'frontend/src/components/analysis/LeadDetail.tsx') {
    summary = '关联线索详情面板，展示单条线索的置信度、匹配信号、来源溯源、支持证据节点与关联簇的完整信息。';
    tags = ['component', 'correlation', 'lead', 'detail'];
  } else if (path === 'frontend/src/components/analysis/LeadList.test.tsx') {
    summary = '关联线索列表和家族覆盖面板的单元测试，验证线索卡片渲染及覆盖率统计逻辑。';
    tags = ['test', 'unit-test', 'lead', 'list'];
  } else if (path === 'frontend/src/components/analysis/LeadList.tsx') {
    summary = '关联线索列表组件，包含家族覆盖率面板（CorrelationFamilyCoveragePanel）和线索卡片（LeadCard），支持线索选择、跳转及置信度可视化。';
    tags = ['component', 'correlation', 'lead', 'list'];
  } else if (path === 'frontend/src/components/analysis/correlation-helpers.tsx') {
    summary = '关联分析工具函数集，提供置信度标签/色调、覆盖率、溯源语句翻译、线索种类摘要、过滤判断及通用 UI 小组件（Metric、FamilyPills、OverviewCard）。';
    tags = ['utility', 'correlation', 'helpers', 'ui'];
  } else if (path === 'frontend/src/components/analysis/panels/BrowserHistoryPanel.test.tsx') {
    summary = '浏览器历史分析面板的单元测试，验证面板组件渲染及 summary 数据的样式展示。';
    tags = ['test', 'unit-test', 'browser', 'history'];
  } else if (path === 'frontend/src/components/analysis/panels/BrowserHistoryPanel.tsx') {
    summary = '浏览器历史分析面板，按浏览器类型分组展示访问记录、下载、Cookie、会话及密码数据，提供多标签页统计视图。';
    tags = ['component', 'analysis', 'browser', 'history', 'panel'];
  } else if (path === 'frontend/src/components/analysis/panels/ClassificationPanel.tsx') {
    summary = '证据分类面板，包含证据分类概览（EvidenceClassificationPanel）、文件分类详情（FileClassificationPanel）和分析报告（AnalysisReportPanel）三个子组件。';
    tags = ['component', 'analysis', 'classification', 'panel'];
  } else if (path === 'frontend/src/components/analysis/panels/EmailExtractionPanel.tsx') {
    summary = '邮件提取分析面板，展示邮件概要统计、邮件详情卡片（含附件、正文预览、邮件头）、HTML 正文渲染和支持多维字段展示。';
    tags = ['component', 'analysis', 'email', 'extraction', 'panel'];
  } else if (path === 'frontend/src/components/analysis/panels/EventLogPanel.test.tsx') {
    summary = '事件日志分析面板的单元测试，覆盖安全事件过滤和面板组件渲染验证。';
    tags = ['test', 'unit-test', 'event-log', 'evtx'];
  } else if (path === 'frontend/src/components/analysis/panels/EventLogPanel.tsx') {
    summary = 'Windows 事件日志（EVTX）分析面板，分类展示安全事件（登录/注销、进程执行、计划任务等）、启动/关机记录及应用崩溃统计。';
    tags = ['component', 'analysis', 'event-log', 'evtx', 'panel'];
  } else if (path === 'frontend/src/components/analysis/panels/RegistryExtractionPanel.test.tsx') {
    summary = '注册表提取分析面板的单元测试，验证多注册表 Hive 数据展示和 UI 渲染。';
    tags = ['test', 'unit-test', 'registry', 'extraction'];
  } else if (path === 'frontend/src/components/analysis/panels/RegistryExtractionPanel.tsx') {
    summary = '注册表提取分析面板，提供 SAM 用户账户、UserAssist 执行历史、USB 设备连接、网络适配器及多 Hive 结构化摘要的多标签页展示。';
    tags = ['component', 'analysis', 'registry', 'extraction', 'panel'];
  } else if (path === 'frontend/src/components/analysis/panels/SystemInfoPanel.tsx') {
    summary = '系统信息面板集合，包含系统概要面板、分析页眉（含提取进度）、空状态引导、错误横幅及加载状态等 5 个子组件。';
    tags = ['component', 'analysis', 'system-info', 'panel'];
  } else if (path === 'frontend/src/components/analysis/panels/helpers.tsx') {
    summary = '分析面板通用工具组件库，提供格式化函数（formatSize、statusLabel）、提取进度组件、表格框架、来源溯源面板、统计卡片、状态标签等 19 个可复用导出。';
    tags = ['utility', 'component', 'helpers', 'analysis', 'panel'];
  } else if (path === 'frontend/src/components/batch/BatchHistory.test.tsx') {
    summary = '批量任务历史组件的单元测试，验证任务数据结构的渲染展示。';
    tags = ['test', 'unit-test', 'batch', 'history'];
  }

  nodes.push({
    id: `file:${path}`,
    type: 'file',
    name,
    filePath: path,
    summary,
    tags,
    complexity
  });
}

// Now add function/class nodes for significant ones
for (const r of results) {
  const path = r.path;

  if (!r.functions) continue;

  for (const fn of r.functions) {
    const lineCount = fn.endLine - fn.startLine + 1;
    const isExported = r.exports && r.exports.some(e => e.name === fn.name);

    // Filter: 10+ lines OR exported (with minimum 3 lines)
    if (lineCount < 3) continue;
    if (lineCount < 10 && !isExported) continue;

    // Special cases - skip test helper functions that are just data constructors
    if (path.includes('.test.') && fn.name !== 'renderPage' && fn.name !== 'queryState' && fn.name !== 'mutationState') {
      // For test files, only include the named setup functions
      if (fn.name.startsWith('make') || fn.name === 'renderWorkspace') {
        // include setup helpers with >= 8 lines
        if (lineCount < 8) continue;
      }
    }

    let summary = '';
    let tags = [];

    // Generate summaries
    if (fn.name === 'TooltipProvider') {
      summary = 'Tooltip 上下文提供者组件，设置全局延迟时间参数（默认 0），包裹 Tooltip 子组件树。';
      tags = ['component', 'tooltip', 'provider', 'context'];
    } else if (fn.name === 'Tooltip') {
      summary = 'Tooltip 根组件，基于 Radix UI Tooltip 原语封装，接收并透传所有属性。';
      tags = ['component', 'tooltip', 'wrapper'];
    } else if (fn.name === 'TooltipTrigger') {
      summary = 'Tooltip 触发器组件，包裹触发悬浮提示的交互元素（如按钮、图标）。';
      tags = ['component', 'tooltip', 'trigger'];
    } else if (fn.name === 'TooltipContent') {
      summary = 'Tooltip 内容弹出层组件，使用 cn 工具合并样式类，支持自定义侧偏移量和 Portal 渲染。';
      tags = ['component', 'tooltip', 'content', 'popover'];
    } else if (fn.name === 'useIsMobile') {
      summary = '响应式移动端检测 Hook，监听 matchMedia 查询变化，返回当前视口宽度是否小于 768px。';
      tags = ['hook', 'responsive', 'mobile', 'detection'];
    } else if (fn.name === 'cn') {
      summary = '类名合并工具函数，结合 clsx（条件类名）和 tailwind-merge（冲突去重）处理 Tailwind CSS 类名字符串。';
      tags = ['utility', 'tailwind', 'css', 'classnames'];
    } else if (fn.name === 'renderPage') {
      summary = 'DataAnalysis 页面测试辅助函数，创建包裹必要 Provider 的组件渲染实例。';
      tags = ['test-helper', 'render', 'setup'];
    } else if (fn.name === 'queryState') {
      summary = 'DataAnalysis 页面测试辅助函数，构造模拟的 React Query 查询状态对象。';
      tags = ['test-helper', 'mock', 'query-state'];
    } else if (fn.name === 'errorMessage') {
      summary = '错误消息提取函数，从 API 错误对象中提取可读的消息字符串，兼容 ApiErrorDto 格式。';
      tags = ['utility', 'error-handling', 'formatting'];
    } else if (fn.name === 'DataAnalysis') {
      summary = '数据分析主页面组件（314 行），管理分析数据源选择、分类扫描、多类别提交流程刷新、提取进度跟踪及 AI 摘要生成与下载。';
      tags = ['component', 'page', 'analysis', 'dashboard'];
    } else if (fn.name === 'mutationState') {
      summary = 'V2Workbench 测试辅助函数，模拟异步变更状态。';
      tags = ['test-helper', 'mock', 'mutation'];
    } else if (fn.name === 'queryState') {
      summary = 'V2Workbench 测试辅助函数，模拟 React Query 查询状态。';
      tags = ['test-helper', 'mock', 'query-state'];
    } else if (fn.name === 'V2Workbench') {
      summary = 'V2 分析工作台页面组件（93 行），整合治理快照面板和关联分析工作区，支持 Demo 案例一键载入。';
      tags = ['component', 'page', 'v2', 'workbench'];
    } else if (fn.name === 'ClusterCard') {
      summary = '关联簇卡片组件，展示单个簇的置信度色调、节点/边/时间线事件计数、涉及家族标签，支持点击跳转。';
      tags = ['component', 'correlation', 'cluster', 'card'];
    } else if (fn.name === 'makeSnapshot') {
      summary = 'CorrelationWorkspace 测试辅助函数，构造关联快照模拟数据。';
      tags = ['test-helper', 'mock', 'snapshot'];
    } else if (fn.name === 'renderWorkspace') {
      summary = 'CorrelationWorkspace 测试辅助函数，渲染带 mock store 的工作区组件。';
      tags = ['test-helper', 'render', 'setup'];
    } else if (fn.name === 'CorrelationWorkspace') {
      summary = '关联分析主工作区组件（381 行），核心分析仪表板：线索搜索/筛选、置信度分组、簇关系图、跳转导航和多维度统计面板。';
      tags = ['component', 'correlation', 'workspace', 'dashboard'];
    } else if (fn.name === 'makeLead') {
      summary = 'Lead 相关测试辅助函数，构造关联线索的模拟数据对象。';
      tags = ['test-helper', 'mock', 'lead'];
    } else if (fn.name === 'makeNode') {
      summary = 'LeadDetail 测试辅助函数，构造线索节点的模拟数据对象。';
      tags = ['test-helper', 'mock', 'node'];
    } else if (fn.name === 'LeadDetailPanel') {
      summary = '关联线索详情面板组件（179 行），展示单条线索的完整信息：置信度、匹配信号、来源溯源、支持证据节点列表、关联簇和风险提示。';
      tags = ['component', 'correlation', 'lead', 'detail'];
    } else if (fn.name === 'NodeSummaryCard') {
      summary = '证据节点摘要卡片组件，展示节点的标签、跳转操作和元数据摘要。';
      tags = ['component', 'correlation', 'node', 'summary'];
    } else if (fn.name === 'makeFamilyCoverage') {
      summary = 'LeadList 测试辅助函数，构造家族覆盖率模拟数据。';
      tags = ['test-helper', 'mock', 'family-coverage'];
    } else if (fn.name === 'CorrelationFamilyCoveragePanel') {
      summary = '关联家族覆盖率面板，按文件家族（Registry、Browser、Email 等）展示线索数量、置信度分布和样本信号。';
      tags = ['component', 'correlation', 'family', 'coverage'];
    } else if (fn.name === 'LeadCard') {
      summary = '关联线索卡片组件，展示单条线索的置信度、支持证据数、种类摘要、匹配信号及跳转入口。';
      tags = ['component', 'correlation', 'lead', 'card'];
    } else if (fn.name === 'confidenceLabel') {
      summary = '置信度标签函数，将置信度分数映射为中文标签（High/Medium/Low → 高/中/低置信度）。';
      tags = ['utility', 'correlation', 'confidence', 'label'];
    } else if (fn.name === 'confidenceTone') {
      summary = '置信度色调函数，将置信度分数映射为 UI 颜色语义（高置信度=绿色，中=橙色，低=灰色）。';
      tags = ['utility', 'correlation', 'confidence', 'color'];
    } else if (fn.name === 'coverageTone') {
      summary = '覆盖率色调函数，将覆盖率值映射为 UI 颜色语义。';
      tags = ['utility', 'correlation', 'coverage', 'color'];
    } else if (fn.name === 'coverageLabel') {
      summary = '覆盖率标签函数，将覆盖率数值映射为中文可读标签。';
      tags = ['utility', 'correlation', 'coverage', 'label'];
    } else if (fn.name === 'translateGuarantee') {
      summary = '溯源保证级别翻译函数，将 guarantee 枚举值（Verified/Plausible/Weak/DirectEvidence 等）映射为中文说明文本。';
      tags = ['utility', 'correlation', 'provenance', 'translation'];
    } else if (fn.name === 'summarizeLeadKinds') {
      summary = '线索种类摘要函数，将线索关联的文件家族和来源标签聚合为唯一可读的摘要字符串。';
      tags = ['utility', 'correlation', 'lead', 'summary'];
    } else if (fn.name === 'isReviewLead') {
      summary = '待审核线索判断函数，检查线索的溯源来源中是否包含 REVIEW 级别项。';
      tags = ['utility', 'correlation', 'filter', 'review'];
    } else if (fn.name === 'isHighConfidenceLead') {
      summary = '高置信度线索判断函数，检查线索置信度分数是否达到高置信度阈值。';
      tags = ['utility', 'correlation', 'filter', 'high-confidence'];
    } else if (fn.name === 'Metric') {
      summary = '通用度量指标展示小组件，渲染标签-值对，用于统计面板。';
      tags = ['component', 'ui', 'metric', 'stat'];
    } else if (fn.name === 'FamilyPills') {
      summary = '文件家族标签丸组件，将多个家族名称渲染为彩色标签丸列表。';
      tags = ['component', 'ui', 'family', 'pills'];
    } else if (fn.name === 'OverviewCard') {
      summary = '概览卡片小组件，带标题的通用内容展示卡片。';
      tags = ['component', 'ui', 'card', 'overview'];
    } else if (fn.name === 'groupByBrowser') {
      summary = '浏览器历史数据分组函数，按浏览器类型（Chrome/Edge/Firefox 等）对记录进行分组。';
      tags = ['utility', 'browser', 'grouping', 'data'];
    } else if (fn.name === 'browserOrder') {
      summary = '浏览器类型排序比较函数，定义浏览器展示优先级和字母序排列规则。';
      tags = ['utility', 'browser', 'sorting', 'ordering'];
    } else if (fn.name === 'BrowserHistoryPanel') {
      summary = '浏览器历史分析面板（232 行），将浏览器的访问/下载/Cookie/会话/密码五类数据按浏览器分组并分标签页展示。';
      tags = ['component', 'analysis', 'browser', 'history', 'panel'];
    } else if (fn.name === 'EvidenceClassificationPanel') {
      summary = '证据分类概览面板，展示分类总数、候选文件数、产出物数量及各类别详情（包含告警和来源信息）。';
      tags = ['component', 'analysis', 'classification', 'evidence'];
    } else if (fn.name === 'FileClassificationPanel') {
      summary = '文件分类详情面板，展示分类统计聚合（扩展名/类别映射）、各类别文件大小及前 20 个文件清单。';
      tags = ['component', 'analysis', 'classification', 'file'];
    } else if (fn.name === 'AnalysisReportPanel') {
      summary = '分析报告面板，展示可下载的 HTML/CSV/JSON 分析报告入口，含文件大小和生成时间信息。';
      tags = ['component', 'analysis', 'report', 'download'];
    } else if (fn.name === 'EmailExtractionPanel') {
      summary = '邮件提取分析面板（128 行），提供邮件概要、搜索过滤、邮件详情卡片展开、附件预览和邮件头查看功能。';
      tags = ['component', 'analysis', 'email', 'extraction', 'panel'];
    } else if (fn.name === 'EmailDetailCard') {
      summary = '邮件详情卡片组件（138 行），展示单封邮件的发件人/收件人/主题/日期、正文预览、附件列表及完整邮件头。';
      tags = ['component', 'email', 'detail', 'card'];
    } else if (fn.name === 'Field') {
      summary = '通用字段展示小组件，渲染标签-值对并支持 onClick 交互。';
      tags = ['component', 'ui', 'field', 'label-value'];
    } else if (fn.name === 'AttachmentBadge') {
      summary = '附件标签徽章组件，展示附件文件名和文件大小的格式化标签。';
      tags = ['component', 'email', 'attachment', 'badge'];
    } else if (fn.name === 'BodyPreview') {
      summary = '邮件正文预览组件，去除空白后展示纯文本正文的前若干字符。';
      tags = ['component', 'email', 'body', 'preview'];
    } else if (fn.name === 'HtmlPreview') {
      summary = 'HTML 邮件正文预览组件，使用 iframe 的 srcdoc 安全渲染 HTML 格式邮件内容。';
      tags = ['component', 'email', 'html', 'preview'];
    } else if (fn.name === 'HeaderList') {
      summary = '邮件头列表组件，将邮件头键值对渲染为可折叠的标签-值列表。';
      tags = ['component', 'email', 'headers', 'list'];
    } else if (fn.name === 'joinAddresses') {
      summary = '邮件地址拼接工具函数，将地址数组用逗号拼接为字符串。';
      tags = ['utility', 'email', 'formatting'];
    } else if (fn.name === 'EventLogPanel') {
      summary = 'Windows 事件日志（EVTX）分析面板（180 行），分类展示安全事件、启动/关机、进程执行、应用崩溃等多维度统计，支持标签页切换。';
      tags = ['component', 'analysis', 'event-log', 'evtx', 'panel'];
    } else if (fn.name === 'RegistryExtractionPanel') {
      summary = '注册表提取分析面板（227 行），分标签页展示 SAM 账户、UserAssist 执行历史、USB 设备、网络适配器及多 Hive 结构化数据。';
      tags = ['component', 'analysis', 'registry', 'extraction', 'panel'];
    } else if (fn.name === 'SystemInfoPanel') {
      summary = '系统信息主面板（105 行），展示系统概要、来源溯源、磁盘容量、网络适配器及启动历史等系统级信息。';
      tags = ['component', 'analysis', 'system-info', 'panel'];
    } else if (fn.name === 'AnalysisHeader') {
      summary = '分析页眉组件（90 行），包含数据源选择器、扫描/提取操作按钮、进度条和产出物统计（扫描文件数/产出物事件数）。';
      tags = ['component', 'analysis', 'header', 'controls'];
    } else if (fn.name === 'AnalysisEmptyState') {
      summary = '分析空状态引导组件，当未选择数据源或未运行提取时显示引导提示和操作入口。';
      tags = ['component', 'analysis', 'empty-state', 'onboarding'];
    } else if (fn.name === 'AnalysisErrorBanner') {
      summary = '分析错误横幅组件，以醒目样式展示分析过程中的错误信息。';
      tags = ['component', 'analysis', 'error', 'banner'];
    } else if (fn.name === 'AnalysisLoadingPanel') {
      summary = '分析加载状态面板，在数据加载期间显示脉动骨架屏。';
      tags = ['component', 'analysis', 'loading', 'skeleton'];
    } else if (fn.name === 'formatSize') {
      summary = '文件大小格式化函数，将字节数转换为 KB/MB/GB 的人类可读格式。';
      tags = ['utility', 'formatting', 'size', 'bytes'];
    } else if (fn.name === 'statusLabel') {
      summary = '状态标签函数，将提取状态枚举值（pending/scanning/extracting/done/error）映射为中文标签和颜色语义。';
      tags = ['utility', 'status', 'label', 'analysis'];
    } else if (fn.name === 'extractionProgressLabel') {
      summary = '提取进度标签函数，根据当前提取进度状态生成可读的阶段描述文本。';
      tags = ['utility', 'progress', 'label', 'extraction'];
    } else if (fn.name === 'AnalysisExtractionProgress') {
      summary = '提取进度面板组件，以进度条和步骤列表形式展示多类别提取的当前状态和告警信息。';
      tags = ['component', 'analysis', 'progress', 'extraction'];
    } else if (fn.name === 'ExtractionTableSection') {
      summary = '提取表格区域组件，为提取数据表格提供标题栏和内容的布局包装。';
      tags = ['component', 'analysis', 'table', 'section'];
    } else if (fn.name === 'SummaryStrip') {
      summary = '摘要信息条组件，水平排列展示一组度量指标的标签-值对。';
      tags = ['component', 'ui', 'summary', 'metrics'];
    } else if (fn.name === 'TableBlock') {
      summary = '表格块组件，为提取结果表格提供标题和内容的容器布局。';
      tags = ['component', 'ui', 'table', 'block'];
    } else if (fn.name === 'DenseTableFrame') {
      summary = '紧凑表格框架组件，提供高密度数据展示的表格样式容器。';
      tags = ['component', 'ui', 'table', 'dense'];
    } else if (fn.name === 'ProvenancePanel') {
      summary = '溯源信息面板组件，展示分析结果的来源路径、提取方法和完整性验证信息。';
      tags = ['component', 'analysis', 'provenance', 'traceability'];
    } else if (fn.name === 'FieldProvenancePanel') {
      summary = '字段级溯源面板，展示每个数据字段级别的来源信息、提取方法和置信度。';
      tags = ['component', 'analysis', 'provenance', 'field-level'];
    } else if (fn.name === 'formatProvenanceSummary') {
      summary = '溯源摘要格式化函数，将溯源数据对象格式化为包含状态标签的可读摘要。';
      tags = ['utility', 'provenance', 'formatting', 'summary'];
    } else if (fn.name === 'RunMetric') {
      summary = '运行度量指标组件，展示单次分析运行的统计键值对。';
      tags = ['component', 'ui', 'metric', 'run-stats'];
    } else if (fn.name === 'InfoCard') {
      summary = '信息卡片组件，带标题、图标的通用信息展示卡片。';
      tags = ['component', 'ui', 'card', 'info'];
    } else if (fn.name === 'StatCard') {
      summary = '统计卡片组件，以大号数字突出展示关键统计指标。';
      tags = ['component', 'ui', 'card', 'stat'];
    } else if (fn.name === 'StatusPill') {
      summary = '状态丸组件，以彩色胶囊形状展示运行状态（完成/进行中/错误等）。';
      tags = ['component', 'ui', 'status', 'pill'];
    } else if (fn.name === 'WarningList') {
      summary = '告警列表组件，以醒目样式展示分析过程中的警告信息列表。';
      tags = ['component', 'ui', 'warning', 'list'];
    } else if (fn.name === 'EmptyLine') {
      summary = '空行占位组件，在无数据时展示提示占位信息。';
      tags = ['component', 'ui', 'empty', 'placeholder'];
    } else if (fn.name === 'makeJob') {
      summary = 'BatchHistory 测试辅助函数，构造批量任务模拟数据。';
      tags = ['test-helper', 'mock', 'job'];
    }

    // Ensure name is sanitized: remove any \r\n etc
    const cleanName = fn.name.replace(/[\r\n]/g, '').trim();

    if (summary) {
      nodes.push({
        id: `function:${path}:${cleanName}`,
        type: 'function',
        name: cleanName,
        filePath: path,
        lineRange: [fn.startLine, fn.endLine],
        summary,
        tags,
        complexity: lineCount > 50 ? 'complex' : lineCount > 20 ? 'moderate' : 'simple'
      });

      // Add contains edge
      edges.push({
        source: `file:${path}`,
        target: `function:${path}:${cleanName}`,
        type: 'contains',
        direction: 'forward',
        weight: 1.0
      });

      // Add exports edge if exported
      if (isExported) {
        edges.push({
          source: `file:${path}`,
          target: `function:${path}:${cleanName}`,
          type: 'exports',
          direction: 'forward',
          weight: 0.8
        });
      }
    }
  }
}

// Add import edges from batchImportData
for (const [filePath, imports] of Object.entries(batchImportData)) {
  for (const importPath of imports) {
    edges.push({
      source: `file:${filePath}`,
      target: `file:${importPath}`,
      type: 'imports',
      direction: 'forward',
      weight: 0.7
    });
  }
}

// Add cross-batch reference edges from neighborMap
for (const [filePath, neighbors] of Object.entries(neighborMap)) {
  for (const neighbor of neighbors) {
    // Only create calls edges if our file uses functions from neighbor
    // For now, add depends_on for runtime dependencies
    if (neighbor.symbols && neighbor.symbols.length > 0) {
      // Add depends_on edge
      edges.push({
        source: `file:${filePath}`,
        target: `file:${neighbor.path}`,
        type: 'depends_on',
        direction: 'forward',
        weight: 0.6
      });
    }
  }
}

// Add calls edges for exported-from-other-batch references
// Based on the call graphs, we can infer cross-batch function calls
// For tooltip.tsx -> cn from utils.ts (both in batch 8 - same batch, skip)
// We'll add calls edges within the batch for known calls
// Example: DataAnalysis.tsx calls useAnalysisSystemInfo from hooks.ts (batch 6)
// V2Workbench calls useV2GovernanceSnapshot from hooks.ts (batch 6)
// Many analysis components call helpers functions

// Since neighborMap tells us which functions each neighbor exports,
// we can add calls edges for known cross-batch calls.
// For simplicity and correctness, I'll add calls edges based on the
// call graph extraction for cross-file references within batch, and
// use neighborMap symbols for cross-batch calls.

// Within-batch calls based on import + call graph
// For example:
// - CorrelationWorkspace -> correlation-helpers.tsx: All helper functions
// - BrowserHistoryPanel -> helpers.tsx: formatSize
// - ClassificationPanel -> helpers.tsx: formatSize, statusLabel, formatProvenanceSummary
// etc.

// These are hard to infer automatically from the call graph alone since
// tree-sitter shows callee names but not the import resolution.
// I'll add the most obvious ones based on function name + imports.

// Add within-batch calls edges where the target file is also in batch 8
// and is clearly imported by the source
const withinBatchCalls = {
  'frontend/src/components/analysis/ClusterView.tsx': {
    'frontend/src/components/analysis/correlation-helpers.tsx': ['confidenceTone', 'confidenceLabel']
  },
  'frontend/src/components/analysis/CorrelationWorkspace.tsx': {
    'frontend/src/components/analysis/correlation-helpers.tsx': ['isHighConfidenceLead', 'isReviewLead', 'confidenceTone', 'confidenceLabel', 'Metric', 'FamilyPills', 'OverviewCard', 'coverageTone', 'coverageLabel', 'summarizeLeadKinds', 'translateGuarantee']
  },
  'frontend/src/components/analysis/LeadDetail.tsx': {
    'frontend/src/components/analysis/correlation-helpers.tsx': ['confidenceTone', 'confidenceLabel', 'translateGuarantee']
  },
  'frontend/src/components/analysis/LeadList.tsx': {
    'frontend/src/components/analysis/correlation-helpers.tsx': ['confidenceTone', 'confidenceLabel', 'coverageTone', 'coverageLabel', 'translateGuarantee', 'summarizeLeadKinds']
  },
  'frontend/src/components/analysis/panels/BrowserHistoryPanel.tsx': {
    'frontend/src/components/analysis/panels/helpers.tsx': ['formatSize']
  },
  'frontend/src/components/analysis/panels/ClassificationPanel.tsx': {
    'frontend/src/components/analysis/panels/helpers.tsx': ['formatSize', 'statusLabel', 'formatProvenanceSummary']
  },
  'frontend/src/components/analysis/panels/EmailExtractionPanel.tsx': {
    'frontend/src/components/analysis/panels/helpers.tsx': ['formatSize']
  },
  'frontend/src/components/analysis/panels/EventLogPanel.tsx': {
    'frontend/src/components/analysis/panels/helpers.tsx': ['DenseTableFrame']
  },
  'frontend/src/components/analysis/panels/RegistryExtractionPanel.tsx': {
    'frontend/src/components/analysis/panels/helpers.tsx': ['DenseTableFrame']
  },
  'frontend/src/components/analysis/panels/SystemInfoPanel.tsx': {
    'frontend/src/components/analysis/panels/helpers.tsx': ['formatSize', 'formatProvenanceSummary', 'AnalysisExtractionProgress']
  },
  'frontend/src/components/analysis/panels/helpers.tsx': {
    'frontend/src/components/analysis/correlation-helpers.tsx': ['Metric'] // Note: helpers.tsx has its own Metric, but correlation-helpers also has one
  }
};

for (const [caller, callees] of Object.entries(withinBatchCalls)) {
  for (const [calleeFile, symbols] of Object.entries(callees)) {
    for (const symbol of symbols) {
      edges.push({
        source: `file:${caller}`,
        target: `function:${calleeFile}:${symbol}`,
        type: 'calls',
        direction: 'forward',
        weight: 0.8
      });
    }
  }
}

// Add tested_by edges (test files exercise the production files they import)
const testProductionPairs = {
  'frontend/src/app/pages/DataAnalysis.test.tsx': 'frontend/src/app/pages/DataAnalysis.tsx',
  'frontend/src/app/pages/V2Workbench.test.tsx': 'frontend/src/app/pages/V2Workbench.tsx',
  'frontend/src/app/routes.test.ts': 'frontend/src/app/routes.tsx',
  'frontend/src/components/analysis/CorrelationWorkspace.test.tsx': 'frontend/src/components/analysis/CorrelationWorkspace.tsx',
  'frontend/src/components/analysis/LeadDetail.test.tsx': 'frontend/src/components/analysis/LeadDetail.tsx',
  'frontend/src/components/analysis/LeadList.test.tsx': 'frontend/src/components/analysis/LeadList.tsx',
  'frontend/src/components/analysis/panels/BrowserHistoryPanel.test.tsx': 'frontend/src/components/analysis/panels/BrowserHistoryPanel.tsx',
  'frontend/src/components/analysis/panels/EventLogPanel.test.tsx': 'frontend/src/components/analysis/panels/EventLogPanel.tsx',
  'frontend/src/components/analysis/panels/RegistryExtractionPanel.test.tsx': 'frontend/src/components/analysis/panels/RegistryExtractionPanel.tsx',
  'frontend/src/components/batch/BatchHistory.test.tsx': 'frontend/src/components/batch/BatchHistory.tsx',
};

for (const [testFile, prodFile] of Object.entries(testProductionPairs)) {
  edges.push({
    source: `file:${prodFile}`,
    target: `file:${testFile}`,
    type: 'tested_by',
    direction: 'forward',
    weight: 0.5
  });
}

// Add AnalysisPanels.tsx as barrel -> it re-exports from panel files and helpers
const barrelExports = [
  'frontend/src/components/analysis/panels/SystemInfoPanel.tsx',
  'frontend/src/components/analysis/panels/BrowserHistoryPanel.tsx',
  'frontend/src/components/analysis/panels/EmailExtractionPanel.tsx',
  'frontend/src/components/analysis/panels/EventLogPanel.tsx',
  'frontend/src/components/analysis/panels/RegistryExtractionPanel.tsx',
  'frontend/src/components/analysis/panels/ClassificationPanel.tsx',
  'frontend/src/components/analysis/panels/helpers.tsx',
];

for (const target of barrelExports) {
  edges.push({
    source: 'file:frontend/src/components/analysis/AnalysisPanels.tsx',
    target: `file:${target}`,
    type: 'exports',
    direction: 'forward',
    weight: 0.8
  });
}

// Add AnalysisPanels re-exports for individual panel components
// AnalysisPanels re-exports: SystemInfoPanel, BrowserHistoryPanel, etc. from their respective files
// These are already covered by imports from DataAnalysis.tsx, V2Workbench.tsx

// Deduplicate edges (sometimes imports + calls create duplicates)
// Use a Set to track unique edges
const edgeSet = new Set();
const dedupedEdges = [];
for (const edge of edges) {
  const key = `${edge.source}|||${edge.target}|||${edge.type}`;
  if (!edgeSet.has(key) && edge.source !== edge.target) {
    edgeSet.add(key);
    dedupedEdges.push(edge);
  }
}

console.log(`Total nodes: ${nodes.length}`);
console.log(`Total edges: ${dedupedEdges.length}`);

// Split into parts if needed
const MAX_NODES = 60;
const MAX_EDGES = 120;

if (nodes.length <= MAX_NODES && dedupedEdges.length <= MAX_EDGES) {
  writeOutput(OUT_DIR, 'batch-8.json', nodes, dedupedEdges);
  console.log('Single output file');
} else {
  const parts = Math.max(
    Math.ceil(nodes.length / MAX_NODES),
    Math.ceil(dedupedEdges.length / MAX_EDGES)
  );
  console.log(`Splitting into ${parts} parts`);

  // Sort files alphabetically
  const sortedFiles = [...allFiles].sort();
  const chunkSize = Math.ceil(sortedFiles.length / parts);

  for (let i = 0; i < parts; i++) {
    const partFiles = new Set(sortedFiles.slice(i * chunkSize, (i + 1) * chunkSize));

    // Nodes whose filePath is in this part
    const partNodes = nodes.filter(n => {
      const fp = n.filePath;
      if (!fp) return true; // include nodes without filePath (none expected)
      return partFiles.has(fp);
    });

    // Edges whose source is in this part's nodes
    const partNodeIds = new Set(partNodes.map(n => n.id));
    const partEdges = dedupedEdges.filter(e => partNodeIds.has(e.source));

    const partName = parts > 1 ? `batch-8-part-${i + 1}.json` : 'batch-8.json';
    writeOutput(OUT_DIR, partName, partNodes, partEdges);
    console.log(`Part ${i + 1}: ${partNodes.length} nodes, ${partEdges.length} edges`);
  }
}

function writeOutput(dir, filename, nodesArr, edgesArr) {
  const outPath = join(dir, filename);
  const output = { nodes: nodesArr, edges: edgesArr };
  writeFileSync(outPath, JSON.stringify(output, null, 2), 'utf8');
  console.log(`Wrote ${filename}: ${nodesArr.length} nodes, ${edgesArr.length} edges`);
}
