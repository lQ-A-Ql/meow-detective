$extraction = Get-Content -Path "D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-11.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$batches = Get-Content -Path "D:/process/forensic/.understand-anything/intermediate/batches.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$batch = $batches.batches | Where-Object { $_.batchIndex -eq 11 }
$importData = $batch.batchImportData

# ========== FILE NODES ==========
$fileNodes = @()
$fileSummaries = @{
    "frontend/src/app/pages/CaseActions.test.tsx" = "CaseActions 组件的单元测试，覆盖案例创建表单和导入区域的渲染与交互行为。"
    "frontend/src/app/pages/CaseActions.tsx" = "案例欢迎表单和导入区域组件，提供案例创建/打开界面及数据源导入操作面板。"
    "frontend/src/app/pages/CaseHome.test.tsx" = "CaseHome 页面的单元测试，验证案例主页的渲染、数据源管理和导入流程。"
    "frontend/src/app/pages/CaseHome.tsx" = "案例主页组件，整合案例概览、数据源管理、导入对话框和案例操作入口。"
    "frontend/src/app/pages/CaseOverview.tsx" = "案例概览面板集，展示案例指标、数据源列表、最近任务和最近对象。"
    "frontend/src/app/pages/Reports.test.tsx" = "Reports 页面的单元测试，验证报告导出界面的渲染。"
    "frontend/src/app/pages/Reports.tsx" = "报告导出页面，支持 CSV/JSON/HTML 格式导出及证据哈希验证状态展示。"
    "frontend/src/app/pages/V3Dashboard.test.tsx" = "V3Dashboard 页面的单元测试，验证分析仪表盘的数据聚合展示。"
    "frontend/src/app/pages/V3Dashboard.tsx" = "V3 分析仪表盘页面，聚合展示图谱、时间线、工件和治理快照数据。"
    "frontend/src/app/pages/V3ScoreCards.tsx" = "仪表盘通用 UI 组件库，包含 StatCard、SectionHeader 和错误信息格式化工具。"
    "frontend/src/app/pages/file-tree-utils.ts" = "文件树工具函数，提供树节点比较和分页数据合并的纯函数。"
    "frontend/src/app/pages/use-file-browser.ts" = "文件浏览器核心 Hook，编排文件树、分页、预览和跳转上下文的组合逻辑。"
    "frontend/src/app/providers.tsx" = "应用级 React Query 和 i18n Provider 包装器，订阅后端投影失效事件自动刷新缓存。"
    "frontend/src/components/analysis/CorrelationPanel.tsx" = "关联分析评分卡面板，展示验证/关联/性能/安全评分及运行时摘要数据。"
    "frontend/src/components/analysis/LimitationsPanel.tsx" = "已知限制面板，按受影响的分析链展示系统当前的能力边界。"
    "frontend/src/components/analysis/V2GovernancePanels.tsx" = "V2 治理面板集合，包含安全审计、错误分类、发布门禁和事实源等子面板。"
    "frontend/src/components/analysis/VerificationPanel.tsx" = "验证面板集合，展示验证仪表盘、基准检查面板和支持矩阵面板。"
    "frontend/src/components/dashboard/ArtifactStatsSection.tsx" = "工件统计仪表盘区块，展示各工件族的检出数量和覆盖率。"
    "frontend/src/components/dashboard/BatchStatusSection.tsx" = "批量作业状态仪表盘区块，展示运行中/完成/失败的批量任务统计。"
    "frontend/src/components/dashboard/CorrelationStatsSection.tsx" = "关联统计仪表盘区块，展示关联规则族覆盖率和线索分布。"
    "frontend/src/components/dashboard/DataSourceCoverageSection.tsx" = "数据源覆盖仪表盘区块，展示各数据源的哈希状态和分区覆盖信息。"
    "frontend/src/components/dashboard/GraphStatsSection.tsx" = "图谱统计仪表盘区块，展示节点/边类型分布及知识图谱规模。"
    "frontend/src/components/dashboard/PlatformCoverageSection.tsx" = "平台覆盖仪表盘区块，按 Windows/Linux/macOS/跨平台展示工件族支持度。"
    "frontend/src/components/dashboard/RulePackStatusSection.tsx" = "规则包状态仪表盘区块，展示已加载分析规则包及其状态。"
    "frontend/src/components/dashboard/TimelineOverviewSection.tsx" = "时间线概览仪表盘区块，展示时间线事件总数和加载状态。"
    "frontend/src/components/gql/GqlEditor.test.tsx" = "GqlEditor 组件的单元测试，验证图谱查询编辑器的渲染。"
    "frontend/src/components/gql/GqlEditor.tsx" = "图谱查询编辑器组件，提供 GQL 查询输入和执行界面。"
    "frontend/src/components/gql/GqlResultView.test.tsx" = "GqlResultView 组件的单元测试，验证图谱查询结果的渲染。"
}

$fileTags = @{
    "frontend/src/app/pages/CaseActions.test.tsx" = @("test", "component-test", "case-management")
    "frontend/src/app/pages/CaseActions.tsx" = @("component", "case-management", "import", "form")
    "frontend/src/app/pages/CaseHome.test.tsx" = @("test", "page-test", "case-management")
    "frontend/src/app/pages/CaseHome.tsx" = @("page", "entry-point", "case-management", "dashboard")
    "frontend/src/app/pages/CaseOverview.tsx" = @("component", "case-management", "dashboard", "overview")
    "frontend/src/app/pages/Reports.test.tsx" = @("test", "component-test", "reporting")
    "frontend/src/app/pages/Reports.tsx" = @("page", "reporting", "export", "evidence")
    "frontend/src/app/pages/V3Dashboard.test.tsx" = @("test", "page-test", "analytics")
    "frontend/src/app/pages/V3Dashboard.tsx" = @("page", "dashboard", "analytics", "aggregation")
    "frontend/src/app/pages/V3ScoreCards.tsx" = @("utility", "component", "dashboard", "ui-primitive")
    "frontend/src/app/pages/file-tree-utils.ts" = @("utility", "file-tree", "comparison", "merge")
    "frontend/src/app/pages/use-file-browser.ts" = @("hook", "file-browser", "orchestration", "state-management")
    "frontend/src/app/providers.tsx" = @("provider", "react-query", "cache-invalidation", "entry-point")
    "frontend/src/components/analysis/CorrelationPanel.tsx" = @("component", "analysis", "scorecard", "correlation")
    "frontend/src/components/analysis/LimitationsPanel.tsx" = @("component", "analysis", "limitations", "governance")
    "frontend/src/components/analysis/V2GovernancePanels.tsx" = @("component", "analysis", "governance", "audit", "barrel")
    "frontend/src/components/analysis/VerificationPanel.tsx" = @("component", "analysis", "verification", "benchmark")
    "frontend/src/components/dashboard/ArtifactStatsSection.tsx" = @("component", "dashboard", "artifacts", "statistics")
    "frontend/src/components/dashboard/BatchStatusSection.tsx" = @("component", "dashboard", "batch", "job-status")
    "frontend/src/components/dashboard/CorrelationStatsSection.tsx" = @("component", "dashboard", "correlation", "statistics")
    "frontend/src/components/dashboard/DataSourceCoverageSection.tsx" = @("component", "dashboard", "data-source", "coverage")
    "frontend/src/components/dashboard/GraphStatsSection.tsx" = @("component", "dashboard", "graph", "statistics")
    "frontend/src/components/dashboard/PlatformCoverageSection.tsx" = @("component", "dashboard", "platform", "coverage")
    "frontend/src/components/dashboard/RulePackStatusSection.tsx" = @("component", "dashboard", "rule-pack", "status")
    "frontend/src/components/dashboard/TimelineOverviewSection.tsx" = @("component", "dashboard", "timeline", "overview")
    "frontend/src/components/gql/GqlEditor.test.tsx" = @("test", "component-test", "graph-query")
    "frontend/src/components/gql/GqlEditor.tsx" = @("component", "graph-query", "editor", "gql")
    "frontend/src/components/gql/GqlResultView.test.tsx" = @("test", "component-test", "graph-query")
}

function Get-Complexity($lines) {
    if ($lines -lt 50) { return "simple" }
    if ($lines -le 200) { return "moderate" }
    return "complex"
}

foreach ($r in $extraction.results) {
    $path = $r.path
    $node = @{
        id = "file:$path"
        type = "file"
        name = [System.IO.Path]::GetFileName($path)
        filePath = $path
        summary = $fileSummaries[$path]
        tags = $fileTags[$path]
        complexity = Get-Complexity $r.nonEmptyLines
    }
    $fileNodes += $node
}

Write-Host "File nodes created: $($fileNodes.Count)"

# ========== FUNCTION NODES ==========
$funcNodes = @()
foreach ($r in $extraction.results) {
    if (-not $r.functions) { continue }
    $path = $r.path
    foreach ($f in $r.functions) {
        $len = $f.endLine - $f.startLine
        $isExported = $false
        if ($r.exports) {
            foreach ($e in $r.exports) {
                if ($e.name -eq $f.name) { $isExported = $true; break }
            }
        }
        # Significance filter: 10+ lines OR exported
        if ($len -lt 10 -and -not $isExported) { continue }
        
        $fpath = $path
        $fname = $f.name
        $fcplx = if ($len -lt 20) { "simple" } elseif ($len -le 80) { "moderate" } else { "complex" }
        
        $fnode = @{
            id = "function:$($fpath):$($fname)"
            type = "function"
            name = $fname
            filePath = $fpath
            lineRange = @($f.startLine, $f.endLine)
            summary = ""
            tags = @()
            complexity = $fcplx
        }
        $funcNodes += $fnode
    }
}

# Now populate summaries and tags for function nodes
$funcNodeMap = @{}
foreach ($fn in $funcNodes) {
    $funcNodeMap[$fn.id] = $fn
}

# Summary and tags for each function
$funcMeta = @{
    "function:frontend/src/app/pages/CaseActions.test.tsx:baseWelcomeProps" = @{summary="构建 CaseWelcomeForms 测试所用的默认属性对象。"; tags=@("test-fixture", "helper")}
    "function:frontend/src/app/pages/CaseActions.test.tsx:baseImportProps" = @{summary="构建 ImportSection 测试所用的默认属性对象。"; tags=@("test-fixture", "helper")}
    "function:frontend/src/app/pages/CaseActions.tsx:CaseWelcomeForms" = @{summary="案例欢迎表单组件，渲染案例创建/打开界面，支持设置案例根目录和名称，列出最近案例并支持删除。"; tags=@("component", "form", "case-creation", "ui")}
    "function:frontend/src/app/pages/CaseActions.tsx:ImportSection" = @{summary="数据源导入区域组件，管理导入路径选择、导入触发、进度展示和失败重试。"; tags=@("component", "import", "data-source", "ui")}
    "function:frontend/src/app/pages/CaseHome.test.tsx:mockQueryState" = @{summary="构建 mock React Query 查询状态的辅助函数。"; tags=@("test-fixture", "mock", "helper")}
    "function:frontend/src/app/pages/CaseHome.test.tsx:mockMutationState" = @{summary="构建 mock React Query 变更状态的辅助函数。"; tags=@("test-fixture", "mock", "helper")}
    "function:frontend/src/app/pages/CaseHome.test.tsx:renderPage" = @{summary="渲染 CaseHome 页面并包裹必要 Provider 的测试辅助函数。"; tags=@("test-fixture", "render", "helper")}
    "function:frontend/src/app/pages/CaseHome.tsx:CaseHome" = @{summary="案例主页顶层组件，编排所有案例管理 Hook（创建/打开/删除/导入/重命名），渲染 CaseOverview 和 CaseActions。"; tags=@("page", "entry-point", "case-management", "orchestration")}
    "function:frontend/src/app/pages/CaseOverview.tsx:MetricBlock" = @{summary="单个指标展示块，渲染带图标、标题和数值的卡片。"; tags=@("component", "metric", "ui")}
    "function:frontend/src/app/pages/CaseOverview.tsx:CaseMetricsStrip" = @{summary="案例指标条，聚合展示数据源数、索引文件数、时间线事件数和工件数。"; tags=@("component", "metric", "dashboard")}
    "function:frontend/src/app/pages/CaseOverview.tsx:RecentTasksPanel" = @{summary="最近任务面板，展示运行中、已完成和部分完成的任务列表。"; tags=@("component", "task", "job-status", "ui")}
    "function:frontend/src/app/pages/CaseOverview.tsx:DataSourcesPanel" = @{summary="数据源管理面板，支持数据源列表展示、内联重命名和删除操作。"; tags=@("component", "data-source", "management", "ui")}
    "function:frontend/src/app/pages/CaseOverview.tsx:RecentObjectsPanel" = @{summary="最近对象面板，展示案例中最近访问的文件和工件列表。"; tags=@("component", "recent-objects", "ui")}
    "function:frontend/src/app/pages/Reports.tsx:Reports" = @{summary="报告导出页面组件，支持选择导出范围（全部/仅工件/仅文件）、导出格式（CSV/JSON/HTML）和证据哈希验证。"; tags=@("page", "export", "reporting", "evidence-hash")}
    "function:frontend/src/app/pages/V3Dashboard.tsx:V3Dashboard" = @{summary="V3 分析仪表盘页面，集中获取并展示图谱、时间线、工件、关联和治理快照数据，支持一键刷新所有模块。"; tags=@("page", "dashboard", "analytics", "aggregation")}
    "function:frontend/src/app/pages/V3ScoreCards.tsx:errorMessage" = @{summary="从 API 错误对象中提取用户可读的错误消息字符串。"; tags=@("utility", "error-handling", "helper")}
    "function:frontend/src/app/pages/V3ScoreCards.tsx:StatCard" = @{summary="通用统计卡片组件，展示标题、数值、副标题和图标。"; tags=@("component", "dashboard", "stat-card", "ui")}
    "function:frontend/src/app/pages/V3ScoreCards.tsx:EmptyPlaceholder" = @{summary="空状态占位组件，当数据不可用时显示友好提示。"; tags=@("component", "empty-state", "ui")}
    "function:frontend/src/app/pages/V3ScoreCards.tsx:SectionHeader" = @{summary="仪表盘区块标题组件，展示图标、标题和副标题。"; tags=@("component", "section-header", "ui")}
    "function:frontend/src/app/pages/file-tree-utils.ts:sameTreeNode" = @{summary="比较两个文件树节点是否相同（基于 id、名称和子节点标识符列表）。"; tags=@("utility", "comparison", "file-tree")}
    "function:frontend/src/app/pages/file-tree-utils.ts:sameTreeNodeList" = @{summary="比较两个文件树节点列表是否逐项相同。"; tags=@("utility", "comparison", "file-tree")}
    "function:frontend/src/app/pages/file-tree-utils.ts:mergeTreeNodePages" = @{summary="合并分页加载的文件树节点，以新数据覆盖已存在节点，保持列表顺序。"; tags=@("utility", "merge", "file-tree", "pagination")}
    "function:frontend/src/app/pages/use-file-browser.ts:useFileBrowser" = @{summary="文件浏览器核心 Hook，组合文件树、分页、选中、预览和跳转功能，管理目录导航和分支展开状态。"; tags=@("hook", "file-browser", "orchestration", "state-management")}
    "function:frontend/src/app/providers.tsx:subscribeToProjectionInvalidations" = @{summary="订阅后端投影失效事件，自动触发对应 React Query 缓存失效，实现前后端数据一致性。"; tags=@("cache-invalidation", "event-subscription", "react-query")}
    "function:frontend/src/app/providers.tsx:AppProviders" = @{summary="应用级 Provider 包装器，组合 React Query、QueryClient、i18n 和投影失效订阅。"; tags=@("provider", "react-query", "i18n", "entry-point")}
    "function:frontend/src/components/analysis/CorrelationPanel.tsx:coverageTone" = @{summary="将覆盖率状态映射为视觉色调标识（good/warning/bad）。"; tags=@("utility", "coverage", "mapping", "helper")}
    "function:frontend/src/components/analysis/CorrelationPanel.tsx:coverageLabel" = @{summary="将覆盖率状态映射为用户可读的中文标签。"; tags=@("utility", "coverage", "label", "helper")}
    "function:frontend/src/components/analysis/CorrelationPanel.tsx:ReleaseScorecardPanel" = @{summary="发布评分卡面板，展示验证/关联/性能/安全评分和按规则族细分的覆盖率数据。"; tags=@("component", "scorecard", "correlation", "analysis")}
    "function:frontend/src/components/analysis/LimitationsPanel.tsx:knownLimitationTone" = @{summary="将已知限制状态映射为视觉色调标识。"; tags=@("utility", "limitation", "mapping", "helper")}
    "function:frontend/src/components/analysis/LimitationsPanel.tsx:knownLimitationLabel" = @{summary="将已知限制状态映射为用户可读的中文标签。"; tags=@("utility", "limitation", "label", "helper")}
    "function:frontend/src/components/analysis/LimitationsPanel.tsx:KnownLimitationsPanel" = @{summary="已知限制面板，展示当前系统各分析链的能力边界和受影响的规则族。"; tags=@("component", "limitations", "governance", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:MessageBlock" = @{summary="通用消息块组件，展示带图标、标题和条目列表的信息区块。"; tags=@("component", "message", "ui")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:releaseGateLabel" = @{summary="将发布门禁状态映射为用户可读的中文标签。"; tags=@("utility", "gate", "label", "helper")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:SecurityAuditPanel" = @{summary="安全审计面板，展示审计事件统计、最近审计条目和审计备注信息。"; tags=@("component", "security", "audit", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:ErrorTaxonomyPanel" = @{summary="错误分类面板，展示按错误类型分组的示例和备注信息。"; tags=@("component", "error", "taxonomy", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:ReleaseGatePanel" = @{summary="发布门禁面板，展示各项发布检查和通过/失败状态。"; tags=@("component", "release-gate", "governance", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:GovernanceOverviewStrip" = @{summary="治理概览条，展示支持矩阵、事实源和运行时信号的汇总统计。"; tags=@("component", "overview", "governance", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:GovernanceFactSourcesPanel" = @{summary="事实源面板，展示治理中引用的各类事实源及其派生输出。"; tags=@("component", "fact-source", "governance", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:GovernanceRuntimeResultsPanel" = @{summary="运行时结果面板，展示运行时检查项及其子检查的通过状态。"; tags=@("component", "runtime", "governance", "analysis")}
    "function:frontend/src/components/analysis/V2GovernancePanels.tsx:SecurityAuditRow" = @{summary="单条安全审计记录行组件，展示操作者、事件类型和时间戳。"; tags=@("component", "security", "audit", "row")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:resultLabel" = @{summary="将验证结果状态映射为用户可读的中文标签。"; tags=@("utility", "verification", "label", "helper")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:maturityLabel" = @{summary="将成熟度等级映射为用户可读的中文标签。"; tags=@("utility", "maturity", "label", "helper")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:guaranteeLabel" = @{summary="将保证等级映射为用户可读的中文标签。"; tags=@("utility", "guarantee", "label", "helper")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:VerificationDashboard" = @{summary="验证仪表盘面板，展示各验证链的结果、成熟度等级和已验证样本数。"; tags=@("component", "verification", "dashboard", "analysis")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:BenchmarkPanel" = @{summary="基准检查面板，展示覆盖/缺失/超额的基准检查项和场景覆盖统计。"; tags=@("component", "benchmark", "verification", "analysis")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:SupportMatrixPanel" = @{summary="支持矩阵面板，展示各文件和文件系统的成熟度等级及已验证样本。"; tags=@("component", "support-matrix", "verification", "analysis")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:benchmarkCheckTone" = @{summary="将基准检查状态映射为视觉色调标识。"; tags=@("utility", "benchmark", "mapping", "helper")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:benchmarkCheckLabel" = @{summary="将基准检查状态映射为用户可读的中文标签。"; tags=@("utility", "benchmark", "label", "helper")}
    "function:frontend/src/components/analysis/VerificationPanel.tsx:BenchmarkRequiredCheckRow" = @{summary="单条基准必要检查行组件，展示检查项的状态和描述。"; tags=@("component", "benchmark", "check", "row")}
    "function:frontend/src/components/dashboard/ArtifactStatsSection.tsx:ArtifactStatsSection" = @{summary="工件统计区块，展示各工件族的检出总数和分布。"; tags=@("component", "dashboard", "artifacts", "statistics")}
    "function:frontend/src/components/dashboard/BatchStatusSection.tsx:BatchStatusSection" = @{summary="批量作业状态区块，展示运行中/完成/失败的批量任务计数。"; tags=@("component", "dashboard", "batch", "status")}
    "function:frontend/src/components/dashboard/CorrelationStatsSection.tsx:CorrelationStatsSection" = @{summary="关联统计区块，展示各关联规则族的检出数量和覆盖率。"; tags=@("component", "dashboard", "correlation", "statistics")}
    "function:frontend/src/components/dashboard/DataSourceCoverageSection.tsx:DataSourceCoverageSection" = @{summary="数据源覆盖区块，展示各数据源的哈希验证状态和分区信息。"; tags=@("component", "dashboard", "data-source", "coverage")}
    "function:frontend/src/components/dashboard/GraphStatsSection.tsx:GraphStatsSection" = @{summary="图谱统计区块，展示知识图谱中节点/边按类型的分布统计。"; tags=@("component", "dashboard", "graph", "statistics")}
    "function:frontend/src/components/dashboard/PlatformCoverageSection.tsx:PlatformCoverageSection" = @{summary="平台覆盖区块，按 Windows/Linux/macOS/跨平台展示工件族的支持程度。"; tags=@("component", "dashboard", "platform", "coverage")}
    "function:frontend/src/components/dashboard/RulePackStatusSection.tsx:RulePackStatusSection" = @{summary="规则包状态区块，展示已加载分析规则包及其启用/禁用状态。"; tags=@("component", "dashboard", "rule-pack", "status")}
    "function:frontend/src/components/dashboard/TimelineOverviewSection.tsx:TimelineOverviewSection" = @{summary="时间线概览区块，展示时间线事件总数和查询加载状态。"; tags=@("component", "dashboard", "timeline", "overview")}
    "function:frontend/src/components/gql/GqlEditor.tsx:GqlEditor" = @{summary="图谱查询编辑器组件，提供 GQL 查询输入框和执行按钮，展示查询结果和加载状态。"; tags=@("component", "graph-query", "editor", "gql")}
}

foreach ($fn in $funcNodes) {
    $meta = $funcMeta[$fn.id]
    if ($meta) {
        $fn.summary = $meta.summary
        $fn.tags = $meta.tags
    }
    else {
        Write-Host "WARNING: Missing meta for $($fn.id)"
    }
}

Write-Host "Function nodes created: $($funcNodes.Count)"

# ========== ALL NODES ==========
$allNodes = @($fileNodes) + @($funcNodes)
Write-Host "Total nodes: $($allNodes.Count)"

# ========== EDGES ==========
$edges = @()

# 1. IMPORT edges - one per batchImportData entry
foreach ($prop in $importData.PSObject.Properties) {
    $sourcePath = $prop.Name
    $targets = $prop.Value
    foreach ($target in $targets) {
        $edges += @{
            source = "file:$sourcePath"
            target = "file:$target"
            type = "imports"
            direction = "forward"
            weight = 0.7
        }
    }
}
Write-Host "Import edges: $($edges.Count)"

# 2. CONTAINS edges - file contains function
foreach ($fn in $funcNodes) {
    $edges += @{
        source = "file:$($fn.filePath)"
        target = $fn.id
        type = "contains"
        direction = "forward"
        weight = 1.0
    }
}
Write-Host "Contains edges: $($funcNodes.Count)"

# 3. EXPORTS edges - file exports function
foreach ($r in $extraction.results) {
    if (-not $r.exports) { continue }
    $path = $r.path
    foreach ($e in $r.exports) {
        $funcId = "function:$($path):$($e.name)"
        if ($funcNodeMap.ContainsKey($funcId)) {
            $edges += @{
                source = "file:$path"
                target = $funcId
                type = "exports"
                direction = "forward"
                weight = 0.8
            }
        }
    }
}
Write-Host "Exports edges added"

# 4. TESTED_BY edges
$testPairs = @(
    @{prod="frontend/src/app/pages/CaseActions.tsx"; test="frontend/src/app/pages/CaseActions.test.tsx"},
    @{prod="frontend/src/app/pages/CaseHome.tsx"; test="frontend/src/app/pages/CaseHome.test.tsx"},
    @{prod="frontend/src/app/pages/Reports.tsx"; test="frontend/src/app/pages/Reports.test.tsx"},
    @{prod="frontend/src/app/pages/V3Dashboard.tsx"; test="frontend/src/app/pages/V3Dashboard.test.tsx"},
    @{prod="frontend/src/components/gql/GqlEditor.tsx"; test="frontend/src/components/gql/GqlEditor.test.tsx"}
)
foreach ($pair in $testPairs) {
    $edges += @{
        source = "file:$($pair.prod)"
        target = "file:$($pair.test)"
        type = "tested_by"
        direction = "forward"
        weight = 0.5
    }
}
Write-Host "Tested_by edges: $($testPairs.Count)"

# Total edges
$totalEdges = $edges.Count
Write-Host "Total edges: $totalEdges"

# ========== SPLIT INTO PARTS ==========
$sortedPaths = $extraction.results | Sort-Object { $_.path } | Select-Object -ExpandProperty path
$parts = [Math]::Ceiling([Math]::Max($allNodes.Count / 60.0, $totalEdges / 120.0))
Write-Host "Parts: $parts"

$partSize = [Math]::Ceiling($sortedPaths.Count / $parts)
Write-Host "Files per part: $partSize"

for ($p = 0; $p -lt $parts; $p++) {
    $start = $p * $partSize
    $end = [Math]::Min(($p + 1) * $partSize, $sortedPaths.Count) - 1
    if ($start -ge $sortedPaths.Count) { break }
    $partFiles = $sortedPaths[$start..$end]
    Write-Host "Part $($p+1): files $($start)..$($end) = $($partFiles -join ', ')"

    # Nodes in this part
    $partNodes = @($allNodes | Where-Object { $_.filePath -in $partFiles })
    $partNodeIds = $partNodes | ForEach-Object { $_.id }
    
    # Edges in this part: source in part nodes
    $partEdges = @($edges | Where-Object { $partNodeIds -contains $_.source })

    $fragment = @{
        nodes = $partNodes
        edges = $partEdges
    }

    $outPath = "D:/process/forensic/.understand-anything/intermediate/batch-4-part-$($p+1).json"
    $json = $fragment | ConvertTo-Json -Depth 6 -Compress
    [System.IO.File]::WriteAllText($outPath, $json, [System.Text.UTF8Encoding]::new($false))
    Write-Host "  Wrote $($partNodes.Count) nodes, $($partEdges.Count) edges to $outPath"
}

Write-Host "DONE"
