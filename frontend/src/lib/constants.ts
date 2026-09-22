/**
 * 前端共享常量
 *
 * 集中管理硬编码的常量值，便于维护和修改。
 */

// ============================================
// 文件树相关
// ============================================

/** 文件树节点高度 (px) */
export const TREE_NODE_HEIGHT = 28;

/** 文件树默认宽度 (px) */
export const TREE_DEFAULT_WIDTH = 224;

/** 文件树最小宽度 (px) */
export const TREE_MIN_WIDTH = 160;

/** 文件树最大宽度 (px) */
export const TREE_MAX_WIDTH = 400;

/** 文件树缓存大小限制 */
export const TREE_CACHE_MAX_SIZE = 100;

/** 虚拟滚动预渲染行数 */
export const VIRTUAL_SCROLL_OVERSCAN = 10;

// ============================================
// 搜索相关
// ============================================

/** 搜索防抖延迟 (ms) */
export const SEARCH_DEBOUNCE_MS = 150;

/** 最大搜索结果数 */
export const MAX_SEARCH_RESULTS = 100;

// ============================================
// 文件大小格式化
// ============================================

/** 文件大小单位 */
export const FILE_SIZE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

/** 文件大小基数 */
export const FILE_SIZE_BASE = 1024;

// ============================================
// localStorage keys
// ============================================

export const STORAGE_KEYS = {
  FILE_SORT_KEY: 'fileSortKey',
  FILE_SORT_DIRECTION: 'fileSortDirection',
  TREE_WIDTH: 'fileTreeWidth',
  SIDEBAR_COLLAPSED: 'sidebarCollapsed',
} as const;
