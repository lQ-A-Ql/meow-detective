/**
 * 文件排序工具 (性能优化版)
 *
 * 优化点：
 * 1. 预计算排序键，避免重复提取
 * 2. 使用更快的字符串比较
 * 3. 减少数组创建
 * 4. 缓存扩展名提取
 */

import { FileEntryRow } from '@/types/models';

/**
 * 排序键类型
 */
export type FileSortKey = 'name' | 'size' | 'modifiedAt' | 'ext' | 'entryType';

/**
 * 排序方向
 */
export type FileSortDirection = 'asc' | 'desc';

/** 预计算的排序键 */
interface SortKeys {
  /** 目录优先标记 (0 = 目录, 1 = 文件) */
  isFile: number;
  /** 名称小写 (用于快速比较) */
  nameLower: string;
  /** 文件大小 */
  size: number;
  /** 修改时间 */
  modifiedAt: string;
  /** 扩展名小写 */
  extLower: string;
}

/**
 * 预计算排序键
 */
function computeSortKeys(row: FileEntryRow): SortKeys {
  const ext = row.ext ?? row.name.split('.').pop() ?? '';
  return {
    isFile: row.entryType === 'directory' ? 0 : 1,
    nameLower: row.name.toLowerCase(),
    size: row.size ?? 0,
    modifiedAt: row.modifiedAt ?? '',
    extLower: ext.toLowerCase(),
  };
}

/**
 * 快速比较两个字符串 (支持数字排序)
 */
function fastCompare(a: string, b: string): number {
  if (a === b) return 0;
  if (a < b) return -1;
  return 1;
}

/**
 * 排序文件列表 (性能优化版)
 *
 * @param rows - 文件列表
 * @param sortKey - 排序键
 * @param direction - 排序方向
 * @returns 排序后的新数组
 */
export function sortFileEntries(
  rows: FileEntryRow[],
  sortKey: FileSortKey = 'name',
  direction: FileSortDirection = 'asc'
): FileEntryRow[] {
  const len = rows.length;
  if (len <= 1) return rows;

  // 预计算所有排序键
  const keysArray: SortKeys[] = new Array(len);
  for (let i = 0; i < len; i++) {
    keysArray[i] = computeSortKeys(rows[i]);
  }

  // 创建索引数组进行排序 (避免移动大对象)
  const indices: number[] = new Array(len);
  for (let i = 0; i < len; i++) {
    indices[i] = i;
  }

  // 根据排序键选择比较函数
  const dirMul = direction === 'asc' ? 1 : -1;

  indices.sort((a, b) => {
    const ka = keysArray[a];
    const kb = keysArray[b];

    // 1. 目录优先
    const fileDiff = ka.isFile - kb.isFile;
    if (fileDiff !== 0) return fileDiff * dirMul;

    // 2. 按指定字段排序
    let cmp = 0;

    switch (sortKey) {
      case 'name':
        cmp = fastCompare(ka.nameLower, kb.nameLower);
        break;
      case 'size':
        cmp = ka.size - kb.size;
        break;
      case 'modifiedAt':
        cmp = fastCompare(ka.modifiedAt, kb.modifiedAt);
        break;
      case 'ext':
        cmp = fastCompare(ka.extLower, kb.extLower);
        break;
      case 'entryType':
        cmp = ka.isFile - kb.isFile;
        break;
    }

    return cmp * dirMul;
  });

  // 根据排序后的索引构建结果
  const result: FileEntryRow[] = new Array(len);
  for (let i = 0; i < len; i++) {
    result[i] = rows[indices[i]];
  }

  return result;
}

/**
 * 切换排序方向
 */
export function toggleSortDirection(current: FileSortDirection): FileSortDirection {
  return current === 'asc' ? 'desc' : 'asc';
}

/**
 * 获取排序键的显示名称
 */
export function getSortKeyLabel(key: FileSortKey): string {
  const labels: Record<FileSortKey, string> = {
    name: '名称',
    size: '大小',
    modifiedAt: '修改时间',
    ext: '扩展名',
    entryType: '类型',
  };
  return labels[key];
}
