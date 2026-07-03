#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const graphPath = process.argv[2];
const outputPath = process.argv[3];

if (!graphPath || !outputPath) {
  console.error('Usage: node ua-arch-assign.js <assembled-graph.json> <layers.json>');
  process.exit(1);
}

const graph = JSON.parse(fs.readFileSync(graphPath, 'utf-8'));

const idToPath = {};
const idToType = {};

graph.nodes.forEach(n => {
  idToPath[n.id] = (n.filePath || '').replace(/\\/g, '/');
  idToType[n.id] = n.type;
});

const fileLevelTypes = new Set(['file', 'config', 'document', 'pipeline', 'table', 'schema', 'resource', 'service', 'endpoint']);
const fileNodes = graph.nodes.filter(n => fileLevelTypes.has(n.type));
const allFileIds = fileNodes.map(n => n.id);

console.error('Assigning ' + allFileIds.length + ' file-level nodes to layers...');

// Layer matchers
function getLayer(fp, type) {
  // persistence - SQL tables first (type='table')
  if (type === 'table') return 'layer:persistence';
  if (fp.startsWith('crates/persistence-sqlite/')) return 'layer:persistence';

  // documentation
  if (fp.startsWith('docs/') || fp.startsWith('development-reports/')) return 'layer:documentation';
  if (type === 'document' && !fp.includes('/')) {
    const rootDocs = ['CLAUDE.md', 'ci.md', 'AGENTS.md', 'README.md', 'PRD.md', 'SECURITY.md',
      'audit-remediation-plan.md', 'autopsy-borrowings.md', 'design.md', 'development-plan.md',
      'frontend-ui-ux.md', 'spec.md', 'test-plan.md'];
    if (rootDocs.includes(path.basename(fp))) return 'layer:documentation';
  }

  // application-services
  if (fp.startsWith('crates/app-services/') || fp.startsWith('crates/ingest/')) return 'layer:application-services';

  // core-foundation
  if (fp.startsWith('crates/domain/') || fp.startsWith('crates/transport/')) return 'layer:core-foundation';

  // desktop-host
  if (fp.startsWith('apps/')) return 'layer:desktop-host';

  // evidence-engine
  const evidenceCrates = [
    'crates/evidence-core/', 'crates/image-e01/', 'crates/image-raw/',
    'crates/fs-ntfs/', 'crates/fs-fat/', 'crates/fs-exfat/', 'crates/fs-ext4/',
    'crates/fs-xfs/', 'crates/fs-btrfs/', 'crates/fs-apfs/', 'crates/fs-hfsplus/',
    'crates/artifacts-windows/', 'crates/artifacts-linux/', 'crates/artifacts-macos/',
    'crates/artifacts-android/', 'crates/artifacts-ios/', 'crates/artifacts-core/',
    'crates/containers-pst/', 'crates/cloud-audit/', 'crates/evtx-patched/'
  ];
  if (evidenceCrates.some(function(p) { return fp.startsWith(p); })) return 'layer:evidence-engine';

  // feature-services
  const featureCrates = [
    'crates/search/', 'crates/timeline/', 'crates/reports/', 'crates/exchange/',
    'crates/gql/', 'crates/mcp-client/', 'crates/updater/', 'crates/crash_handler/'
  ];
  if (featureCrates.some(function(p) { return fp.startsWith(p); })) return 'layer:feature-services';

  // frontend
  if (fp.startsWith('frontend/')) return 'layer:frontend';

  // infrastructure (catch-all for remaining)
  return 'layer:infrastructure';
}

const layerBuckets = {
  'layer:application-services': [],
  'layer:core-foundation': [],
  'layer:desktop-host': [],
  'layer:documentation': [],
  'layer:evidence-engine': [],
  'layer:feature-services': [],
  'layer:frontend': [],
  'layer:infrastructure': [],
  'layer:persistence': []
};

const layerNames = {
  'layer:application-services': '应用服务层',
  'layer:core-foundation': '核心基础层',
  'layer:desktop-host': '桌面宿主层',
  'layer:documentation': '文档层',
  'layer:evidence-engine': '证据引擎层',
  'layer:feature-services': '功能服务层',
  'layer:frontend': '前端表示层',
  'layer:infrastructure': '基础设施层',
  'layer:persistence': '持久化层'
};

const layerDescriptions = {
  'layer:application-services': '用例编排层，负责案件管理、数据源导入管线、文件枚举与分析、关联分析（Correlation）、实体解析（Entity Resolution）、治理评分（Governance）、作业调度、批处理及笔记本功能的核心业务流程',
  'layer:core-foundation': '领域实体定义（Case、DataSource、FileEntry、Artifact、Job 等核心类型）与传输契约（IPC DTO、命令类型、事件主题、分页结构、API 错误形态），是前后端 IPC 边界的唯一真相来源',
  'layer:desktop-host': 'Tauri 2 桌面应用宿主，包含命令处理器（Tauri commands）、事件收发（EventBus）、应用状态管理（AppState）、媒体协议注册（evidence-media:）、平台安全、缓存失效策略及构建配置',
  'layer:documentation': '项目文档体系，包含架构设计文档、开发计划与报告、产品需求、测试计划、CI 设计、安全策略、审计报告、会话记录及 Obsidian 知识库配置',
  'layer:evidence-engine': '只读证据读取与解析引擎，包含磁盘镜像格式支持（RAW、E01）、多文件系统解析器（NTFS、FAT、exFAT、ext4、XFS、Btrfs、APFS、HFS+）、Windows/Linux/macOS/iOS/Android 制品解析器、Windows 注册表解析、EVTX 事件日志、PST/OST 邮件容器解析及云审计日志解析',
  'layer:feature-services': '专项功能模块，包含 Tantivy 全文搜索引擎、时间线事件投影与展示、多格式报告导出（HTML、CSV、JSON、证据包）、STIX 2.1 情报交换（Ed25519 签名、链式保管）、GQL 查询引擎、MCP 客户端集成（SSE/Stdio 传输）、应用更新器及崩溃处理',
  'layer:frontend': 'React/TypeScript 用户界面，包含页面组件（pages）、UI 组件库（components）、状态管理（Zustand stores）、React Query 数据钩子（features）、API 客户端封装（lib/api）、类型定义（types）、国际化（i18n）及样式（styles），通过 Tauri IPC 命令和事件与后端交互',
  'layer:infrastructure': '跨切面工程支撑，包含 CI/CD 流水线（GitHub Actions）、根级项目配置（Cargo workspace、依赖审计、工具链）、PowerShell 守卫脚本、测试资源与夹具（testdata）、跨切面工具库（日志、哈希、文件系统工具、时钟、运行时缓存）、共享测试基础设施及构建辅助脚本',
  'layer:persistence': 'SQLite 数据库仓储层，包含连接管理（WAL 模式、外键约束、超时配置）、数据迁移脚本（分区分片迁移）、表定义（table 类型节点）及针对案件、文件、图、时间线、审计等实体的 CRUD 仓库实现'
};

allFileIds.forEach(function(id) {
  const fp = idToPath[id] || '';
  const type = idToType[id] || '';
  const layer = getLayer(fp, type);
  layerBuckets[layer].push(id);
});

// Verify
const output = [];
var totalAssigned = 0;
Object.keys(layerBuckets).forEach(function(layerId) {
  const count = layerBuckets[layerId].length;
  console.error('  ' + layerId + ': ' + count + ' files');
  totalAssigned += count;
  if (count > 0) {
    output.push({
      id: layerId,
      name: layerNames[layerId],
      description: layerDescriptions[layerId],
      nodeIds: layerBuckets[layerId]
    });
  }
});

console.error('Total assigned: ' + totalAssigned + ' / ' + allFileIds.length);

if (totalAssigned !== allFileIds.length) {
  console.error('MISMATCH! ' + (allFileIds.length - totalAssigned) + ' files unaccounted.');
  process.exit(1);
}

fs.writeFileSync(outputPath, JSON.stringify(output, null, 2), 'utf-8');
console.error('Layers (non-empty): ' + output.length);
console.error('Written to ' + outputPath);
process.exit(0);
