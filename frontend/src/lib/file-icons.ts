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
import { fileIconColor } from '@/design/file-icon-tokens';

export interface FileIconInfo {
  icon: LucideIcon;
  color: string;
}

/**
 * 扩展名 -> 图标映射表
 */
const EXTENSION_ICON_MAP: Record<string, FileIconInfo> = {
  // 可执行文件
  exe: { icon: Terminal, color: fileIconColor('red') },
  dll: { icon: Terminal, color: fileIconColor('red') },
  bat: { icon: Terminal, color: fileIconColor('red') },
  cmd: { icon: Terminal, color: fileIconColor('red') },
  msi: { icon: Terminal, color: fileIconColor('red') },
  com: { icon: Terminal, color: fileIconColor('red') },
  scr: { icon: Terminal, color: fileIconColor('red') },
  ps1: { icon: Terminal, color: fileIconColor('red') },
  sh: { icon: Terminal, color: fileIconColor('green') },

  // 文档
  txt: { icon: FileText, color: fileIconColor('blue') },
  doc: { icon: FileText, color: fileIconColor('blue') },
  docx: { icon: FileText, color: fileIconColor('blue') },
  pdf: { icon: FileText, color: fileIconColor('red') },
  rtf: { icon: FileText, color: fileIconColor('blue') },
  odt: { icon: FileText, color: fileIconColor('blue') },
  md: { icon: FileText, color: fileIconColor('blue') },
  csv: { icon: FileText, color: fileIconColor('emerald') },
  xls: { icon: FileText, color: fileIconColor('emerald') },
  xlsx: { icon: FileText, color: fileIconColor('emerald') },

  // 代码
  js: { icon: FileCode, color: fileIconColor('yellow') },
  jsx: { icon: FileCode, color: fileIconColor('yellow') },
  ts: { icon: FileCode, color: fileIconColor('blue') },
  tsx: { icon: FileCode, color: fileIconColor('blue') },
  py: { icon: FileCode, color: fileIconColor('green') },
  rs: { icon: FileCode, color: fileIconColor('orange') },
  go: { icon: FileCode, color: fileIconColor('cyan') },
  java: { icon: FileCode, color: fileIconColor('red') },
  c: { icon: FileCode, color: fileIconColor('neutral') },
  cpp: { icon: FileCode, color: fileIconColor('neutral') },
  h: { icon: FileCode, color: fileIconColor('neutral') },
  html: { icon: FileCode, color: fileIconColor('red') },
  htm: { icon: FileCode, color: fileIconColor('red') },
  css: { icon: FileCode, color: fileIconColor('blue') },
  scss: { icon: FileCode, color: fileIconColor('red') },
  less: { icon: FileCode, color: fileIconColor('cobalt') },
  json: { icon: FileCode, color: fileIconColor('yellow') },
  xml: { icon: FileCode, color: fileIconColor('orange') },
  yaml: { icon: FileCode, color: fileIconColor('orange') },
  yml: { icon: FileCode, color: fileIconColor('orange') },
  toml: { icon: FileCode, color: fileIconColor('orange') },
  ini: { icon: Settings, color: fileIconColor('muted') },
  cfg: { icon: Settings, color: fileIconColor('muted') },
  conf: { icon: Settings, color: fileIconColor('muted') },

  // 图片
  jpg: { icon: Image, color: fileIconColor('green') },
  jpeg: { icon: Image, color: fileIconColor('green') },
  png: { icon: Image, color: fileIconColor('green') },
  gif: { icon: Image, color: fileIconColor('green') },
  bmp: { icon: Image, color: fileIconColor('green') },
  svg: { icon: Image, color: fileIconColor('green') },
  ico: { icon: Image, color: fileIconColor('green') },
  webp: { icon: Image, color: fileIconColor('green') },
  tiff: { icon: Image, color: fileIconColor('green') },
  tif: { icon: Image, color: fileIconColor('green') },
  psd: { icon: Image, color: fileIconColor('ocean') },

  // 压缩包
  zip: { icon: Archive, color: fileIconColor('amber') },
  rar: { icon: Archive, color: fileIconColor('amber') },
  '7z': { icon: Archive, color: fileIconColor('amber') },
  tar: { icon: Archive, color: fileIconColor('amber') },
  gz: { icon: Archive, color: fileIconColor('amber') },
  bz2: { icon: Archive, color: fileIconColor('amber') },
  xz: { icon: Archive, color: fileIconColor('amber') },
  cab: { icon: Archive, color: fileIconColor('amber') },
  iso: { icon: Archive, color: fileIconColor('violet') },

  // 视频
  mp4: { icon: FileVideo, color: fileIconColor('purple') },
  avi: { icon: FileVideo, color: fileIconColor('purple') },
  mkv: { icon: FileVideo, color: fileIconColor('purple') },
  mov: { icon: FileVideo, color: fileIconColor('purple') },
  wmv: { icon: FileVideo, color: fileIconColor('purple') },
  flv: { icon: FileVideo, color: fileIconColor('purple') },
  webm: { icon: FileVideo, color: fileIconColor('purple') },

  // 音频
  mp3: { icon: FileAudio, color: fileIconColor('teal') },
  wav: { icon: FileAudio, color: fileIconColor('teal') },
  flac: { icon: FileAudio, color: fileIconColor('teal') },
  aac: { icon: FileAudio, color: fileIconColor('teal') },
  ogg: { icon: FileAudio, color: fileIconColor('teal') },
  wma: { icon: FileAudio, color: fileIconColor('teal') },
  m4a: { icon: FileAudio, color: fileIconColor('teal') },

  // 数据库
  db: { icon: Database, color: fileIconColor('slate') },
  sqlite: { icon: Database, color: fileIconColor('slate') },
  sqlite3: { icon: Database, color: fileIconColor('slate') },
  mdb: { icon: Database, color: fileIconColor('slate') },
  accdb: { icon: Database, color: fileIconColor('slate') },
  sql: { icon: Database, color: fileIconColor('slate') },

  // 系统/配置
  sys: { icon: Settings, color: fileIconColor('muted') },
  drv: { icon: Settings, color: fileIconColor('muted') },
  log: { icon: FileText, color: fileIconColor('soft') },
  tmp: { icon: FileText, color: fileIconColor('faint') },
  temp: { icon: FileText, color: fileIconColor('faint') },
  bak: { icon: FileText, color: fileIconColor('soft') },
  old: { icon: FileText, color: fileIconColor('soft') },

  // 取证相关
  evtx: { icon: Database, color: fileIconColor('dark') },
  pf: { icon: FileCode, color: fileIconColor('orange') },
  lnk: { icon: FileCode, color: fileIconColor('blue') },
  dat: { icon: Database, color: fileIconColor('slate') },
  reg: { icon: Database, color: fileIconColor('red') },
  e01: { icon: Archive, color: fileIconColor('violet') },
  raw: { icon: Database, color: fileIconColor('neutral') },
  img: { icon: Database, color: fileIconColor('neutral') },
};

/**
 * 根据文件信息获取图标
 */
export function getFileIcon(node: {
  name: string;
  entryType?: string;
  status?: string;
  expanded?: boolean;
}): FileIconInfo {
  // 目录特殊处理
  if (node.entryType === 'directory') {
    // 加密分区
    if (node.status === 'locked') {
      return { icon: Lock, color: fileIconColor('orange') };
    }
    // 不支持的分区
    if (node.status === 'unsupported') {
      return { icon: HelpCircle, color: fileIconColor('faint') };
    }
    // 普通目录 (展开/折叠)
    return { icon: node.expanded ? FolderOpen : Folder, color: fileIconColor('default') };
  }

  // 文件 - 根据扩展名
  const ext = node.name.split('.').pop()?.toLowerCase() ?? '';
  return EXTENSION_ICON_MAP[ext] ?? { icon: File, color: fileIconColor('default') };
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
