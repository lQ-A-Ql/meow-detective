/**
 * 文件图标映射系统
 *
 * 根据文件类型和扩展名返回对应的图标和颜色。
 */

import {
  File,
  Folder,
  FolderOpen,
  Lock,
  HelpCircle,
  Terminal,
  FileText,
  Image,
  Archive,
  FileCode,
  FileVideo,
  FileAudio,
  Database,
  Settings,
  type LucideIcon,
} from 'lucide-react';

export interface FileIconInfo {
  icon: LucideIcon;
  color: string;
}

/**
 * 扩展名 -> 图标映射表
 */
const EXTENSION_ICON_MAP: Record<string, FileIconInfo> = {
  // 可执行文件
  exe: { icon: Terminal, color: '#e74c3c' },
  dll: { icon: Terminal, color: '#e74c3c' },
  bat: { icon: Terminal, color: '#e74c3c' },
  cmd: { icon: Terminal, color: '#e74c3c' },
  msi: { icon: Terminal, color: '#e74c3c' },
  com: { icon: Terminal, color: '#e74c3c' },
  scr: { icon: Terminal, color: '#e74c3c' },
  ps1: { icon: Terminal, color: '#e74c3c' },
  sh: { icon: Terminal, color: '#2ecc71' },

  // 文档
  txt: { icon: FileText, color: '#3498db' },
  doc: { icon: FileText, color: '#3498db' },
  docx: { icon: FileText, color: '#3498db' },
  pdf: { icon: FileText, color: '#e74c3c' },
  rtf: { icon: FileText, color: '#3498db' },
  odt: { icon: FileText, color: '#3498db' },
  md: { icon: FileText, color: '#3498db' },
  csv: { icon: FileText, color: '#27ae60' },
  xls: { icon: FileText, color: '#27ae60' },
  xlsx: { icon: FileText, color: '#27ae60' },

  // 代码
  js: { icon: FileCode, color: '#f1c40f' },
  jsx: { icon: FileCode, color: '#f1c40f' },
  ts: { icon: FileCode, color: '#3498db' },
  tsx: { icon: FileCode, color: '#3498db' },
  py: { icon: FileCode, color: '#2ecc71' },
  rs: { icon: FileCode, color: '#e67e22' },
  go: { icon: FileCode, color: '#00acd7' },
  java: { icon: FileCode, color: '#e74c3c' },
  c: { icon: FileCode, color: '#555' },
  cpp: { icon: FileCode, color: '#555' },
  h: { icon: FileCode, color: '#555' },
  html: { icon: FileCode, color: '#e74c3c' },
  htm: { icon: FileCode, color: '#e74c3c' },
  css: { icon: FileCode, color: '#3498db' },
  scss: { icon: FileCode, color: '#e74c3c' },
  less: { icon: FileCode, color: '#2965f1' },
  json: { icon: FileCode, color: '#f1c40f' },
  xml: { icon: FileCode, color: '#e67e22' },
  yaml: { icon: FileCode, color: '#e67e22' },
  yml: { icon: FileCode, color: '#e67e22' },
  toml: { icon: FileCode, color: '#e67e22' },
  ini: { icon: Settings, color: '#7f8c8d' },
  cfg: { icon: Settings, color: '#7f8c8d' },
  conf: { icon: Settings, color: '#7f8c8d' },

  // 图片
  jpg: { icon: Image, color: '#2ecc71' },
  jpeg: { icon: Image, color: '#2ecc71' },
  png: { icon: Image, color: '#2ecc71' },
  gif: { icon: Image, color: '#2ecc71' },
  bmp: { icon: Image, color: '#2ecc71' },
  svg: { icon: Image, color: '#2ecc71' },
  ico: { icon: Image, color: '#2ecc71' },
  webp: { icon: Image, color: '#2ecc71' },
  tiff: { icon: Image, color: '#2ecc71' },
  tif: { icon: Image, color: '#2ecc71' },
  psd: { icon: Image, color: '#2980b9' },

  // 压缩包
  zip: { icon: Archive, color: '#f39c12' },
  rar: { icon: Archive, color: '#f39c12' },
  '7z': { icon: Archive, color: '#f39c12' },
  tar: { icon: Archive, color: '#f39c12' },
  gz: { icon: Archive, color: '#f39c12' },
  bz2: { icon: Archive, color: '#f39c12' },
  xz: { icon: Archive, color: '#f39c12' },
  cab: { icon: Archive, color: '#f39c12' },
  iso: { icon: Archive, color: '#8e44ad' },

  // 视频
  mp4: { icon: FileVideo, color: '#9b59b6' },
  avi: { icon: FileVideo, color: '#9b59b6' },
  mkv: { icon: FileVideo, color: '#9b59b6' },
  mov: { icon: FileVideo, color: '#9b59b6' },
  wmv: { icon: FileVideo, color: '#9b59b6' },
  flv: { icon: FileVideo, color: '#9b59b6' },
  webm: { icon: FileVideo, color: '#9b59b6' },

  // 音频
  mp3: { icon: FileAudio, color: '#1abc9c' },
  wav: { icon: FileAudio, color: '#1abc9c' },
  flac: { icon: FileAudio, color: '#1abc9c' },
  aac: { icon: FileAudio, color: '#1abc9c' },
  ogg: { icon: FileAudio, color: '#1abc9c' },
  wma: { icon: FileAudio, color: '#1abc9c' },
  m4a: { icon: FileAudio, color: '#1abc9c' },

  // 数据库
  db: { icon: Database, color: '#34495e' },
  sqlite: { icon: Database, color: '#34495e' },
  sqlite3: { icon: Database, color: '#34495e' },
  mdb: { icon: Database, color: '#34495e' },
  accdb: { icon: Database, color: '#34495e' },
  sql: { icon: Database, color: '#34495e' },

  // 系统/配置
  sys: { icon: Settings, color: '#7f8c8d' },
  drv: { icon: Settings, color: '#7f8c8d' },
  log: { icon: FileText, color: '#95a5a6' },
  tmp: { icon: FileText, color: '#bdc3c7' },
  temp: { icon: FileText, color: '#bdc3c7' },
  bak: { icon: FileText, color: '#95a5a6' },
  old: { icon: FileText, color: '#95a5a6' },

  // 取证相关
  evtx: { icon: Database, color: '#2c3e50' },
  pf: { icon: FileCode, color: '#e67e22' },
  lnk: { icon: FileCode, color: '#3498db' },
  dat: { icon: Database, color: '#34495e' },
  reg: { icon: Database, color: '#e74c3c' },
  e01: { icon: Archive, color: '#8e44ad' },
  raw: { icon: Database, color: '#555' },
  img: { icon: Database, color: '#555' },
};

/**
 * 根据文件信息获取图标
 */
export function getFileIcon(node: {
  name: string;
  entryType?: string;
  status?: string;
  expanded?: boolean;
  deleted?: boolean;
}): FileIconInfo {
  // 目录特殊处理
  if (node.entryType === 'directory') {
    // 加密分区
    if (node.status === 'locked') {
      return { icon: Lock, color: '#e67e22' };
    }
    // 不支持的分区
    if (node.status === 'unsupported') {
      return { icon: HelpCircle, color: '#bdc3c7' };
    }
    // 普通目录 (展开/折叠)
    return { icon: node.expanded ? FolderOpen : Folder, color: '#888' };
  }

  // 已删除文件
  if (node.deleted) {
    return { icon: File, color: '#95a5a6' };
  }

  // 文件 - 根据扩展名
  const ext = node.name.split('.').pop()?.toLowerCase() ?? '';
  return EXTENSION_ICON_MAP[ext] ?? { icon: File, color: '#888' };
}

/**
 * 获取文件类型标签
 */
export function getFileTypeLabel(node: {
  name: string;
  entryType?: string;
}): string {
  if (node.entryType === 'directory') {
    return '目录';
  }

  const ext = node.name.split('.').pop()?.toLowerCase();
  if (!ext) return '文件';

  const labelMap: Record<string, string> = {
    exe: '可执行文件',
    dll: '动态链接库',
    txt: '文本文件',
    pdf: 'PDF 文档',
    doc: 'Word 文档',
    docx: 'Word 文档',
    jpg: '图片',
    jpeg: '图片',
    png: '图片',
    gif: '图片',
    zip: '压缩包',
    rar: '压缩包',
    '7z': '压缩包',
    mp4: '视频',
    mp3: '音频',
    db: '数据库',
    sqlite: '数据库',
  };

  return labelMap[ext] ?? `${ext.toUpperCase()} 文件`;
}
