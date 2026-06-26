/**
 * 文件排序工具
 *
 * 与后端 `app-services/src/file_service` 的比较器保持同构：
 * 1. 目录优先（固定，不受排序方向影响）
 * 2. 状态后置（固定）：正常 < 隐藏/系统 < 删除 < 隐藏系统+删除
 * 3. 指定字段排序（受方向影响）
 * 4. 名称自然排序兜底（始终升序，file2 < file10）
 *
 * 真实链路排序由后端负责；此模块用于前端展示兜底排序与测试。
 */

import { FileEntryRow } from '@/types/models';

/**
 * 排序键类型
 */
export type FileSortKey = 'name' | 'size' | 'modifiedAt' | 'ext';

/**
 * 排序方向
 */
export type FileSortDirection = 'asc' | 'desc';

/** 预计算的排序键 */
interface SortKeys {
  /** 目录优先标记 (0 = 目录, 1 = 文件) */
  typeRank: number;
  /** 状态分桶 (0 正常 / 1 隐藏系统 / 2 删除 / 3 隐藏系统+删除) */
  statusBucket: number;
  /** 名称 (原始，用于自然排序) */
  name: string;
  /** 文件大小 */
  size: number;
  /** 修改时间 */
  modifiedAt: string;
  /** 扩展名小写 */
  extLower: string;
}

function statusBucket(row: FileEntryRow): number {
  const abnormal = Boolean(row.hidden || row.system);
  const deleted = Boolean(row.deleted);
  if (abnormal && deleted) return 3;
  if (deleted) return 2;
  if (abnormal) return 1;
  return 0;
}

/**
 * 预计算排序键
 */
function computeSortKeys(row: FileEntryRow): SortKeys {
  const ext = row.ext ?? row.name.split('.').pop() ?? '';
  return {
    typeRank: row.entryType === 'directory' ? 0 : 1,
    statusBucket: statusBucket(row),
    name: row.name,
    size: row.size ?? 0,
    modifiedAt: row.modifiedAt ?? '',
    extLower: ext.toLowerCase(),
  };
}

/**
 * Windows 风格自然排序：大小写不敏感，连续数字段按数值比较，
 * 使 `file2` 排在 `file10` 之前。
 */
export function naturalCompare(a: string, b: string): number {
  const re = /(\d+|\D+)/g;
  const ax = a.match(re) ?? [];
  const bx = b.match(re) ?? [];
  const len = Math.min(ax.length, bx.length);

  for (let i = 0; i < len; i += 1) {
    const as = ax[i];
    const bs = bx[i];
    const aNum = /^\d/.test(as);
    const bNum = /^\d/.test(bs);

    if (aNum && bNum) {
      const at = as.replace(/^0+/, '');
      const bt = bs.replace(/^0+/, '');
      if (at.length !== bt.length) return at.length - bt.length;
      if (at !== bt) return at < bt ? -1 : 1;
      if (as.length !== bs.length) return as.length - bs.length;
    } else {
      const al = as.toLowerCase();
      const bl = bs.toLowerCase();
      if (al !== bl) return al < bl ? -1 : 1;
      if (as !== bs) return as < bs ? -1 : 1;
    }
  }

  return ax.length - bx.length;
}

/**
 * 排序文件列表
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

  const dirMul = direction === 'asc' ? 1 : -1;

  indices.sort((a, b) => {
    const ka = keysArray[a];
    const kb = keysArray[b];

    // 1. 目录优先 (固定，不随方向翻转)
    if (ka.typeRank !== kb.typeRank) return ka.typeRank - kb.typeRank;

    // 2. 状态后置 (固定，不随方向翻转)
    if (ka.statusBucket !== kb.statusBucket) {
      return ka.statusBucket - kb.statusBucket;
    }

    // 3. 按指定字段排序 (受方向影响)
    let cmp = 0;
    switch (sortKey) {
      case 'name':
        cmp = naturalCompare(ka.name, kb.name);
        break;
      case 'size':
        cmp = ka.size - kb.size;
        break;
      case 'modifiedAt':
        cmp = ka.modifiedAt < kb.modifiedAt ? -1 : ka.modifiedAt > kb.modifiedAt ? 1 : 0;
        break;
      case 'ext':
        cmp = naturalCompare(ka.extLower, kb.extLower);
        break;
    }

    if (cmp !== 0) return cmp * dirMul;

    // 4. 名称自然排序兜底 (始终升序)
    return naturalCompare(ka.name, kb.name);
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
  };
  return labels[key];
}
