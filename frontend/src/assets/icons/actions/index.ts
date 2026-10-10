import arrowUp from './arrow-up.svg';
import arrowLeft from './arrow-left.svg';
import chevronUp from './chevron-up.svg';
import chevronDown from './chevron-down.svg';
import chevronLeft from './chevron-left.svg';
import chevronRight from './chevron-right.svg';
import close from './close.svg';
import check from './check.svg';
import plus from './plus.svg';
import refresh from './refresh.svg';
import download from './download.svg';
import upload from './upload.svg';
import play from './play.svg';
import pause from './pause.svg';
import save from './save.svg';
import trash from './trash.svg';
import pencil from './pencil.svg';
import copy from './copy.svg';
import eye from './eye.svg';
import eyeOff from './eye-off.svg';
import hardDrive from './hard-drive.svg';
import folderOpen from './folder-open.svg';
import folder from './folder.svg';
import file from './file.svg';
import hash from './hash.svg';
import hashMd5 from './hash-md5.svg';
import hashSha1 from './hash-sha1.svg';
import hashSha256 from './hash-sha256.svg';
import hashSm3 from './hash-sm3.svg';
import filter from './filter.svg';
import history from './history.svg';
import restore from './restore.svg';
import lock from './lock.svg';
import unlock from './unlock.svg';
import erase from './erase.svg';
import plug from './plug.svg';
import volume from './volume.svg';
import help from './help.svg';
import selectAll from './select-all.svg';
import info from './info.svg';
import warning from './warning.svg';
import search from './search.svg';
import shield from './shield.svg';
import shieldCheck from './shield-check.svg';
import clock from './clock.svg';
import list from './list.svg';
import maximize from './maximize.svg';
import minimize from './minimize.svg';
import zoomIn from './zoom-in.svg';
import zoomOut from './zoom-out.svg';
import fit from './fit.svg';
import share from './share.svg';
import fileDownload from './file-download.svg';
import key from './key.svg';
import memory from './memory.svg';

/** Action artwork, drawn for this product: 24 grid, 1.5px stroke, square caps, outline only. */
export const actionIconAssets = {
  'arrow-up': arrowUp,
  'arrow-left': arrowLeft,
  'chevron-up': chevronUp,
  'chevron-down': chevronDown,
  'chevron-left': chevronLeft,
  'chevron-right': chevronRight,
  close,
  check,
  plus,
  refresh,
  download,
  upload,
  play,
  pause,
  save,
  trash,
  pencil,
  copy,
  eye,
  'eye-off': eyeOff,
  'hard-drive': hardDrive,
  'folder-open': folderOpen,
  folder,
  file,
  hash,
  'hash-md5': hashMd5,
  'hash-sha1': hashSha1,
  'hash-sha256': hashSha256,
  'hash-sm3': hashSm3,
  filter,
  history,
  restore,
  lock,
  unlock,
  erase,
  plug,
  volume,
  help,
  'select-all': selectAll,
  info,
  warning,
  search,
  shield,
  'shield-check': shieldCheck,
  clock,
  list,
  maximize,
  minimize,
  'zoom-in': zoomIn,
  'zoom-out': zoomOut,
  fit,
  share,
  'file-download': fileDownload,
  key,
  memory,
} as const;

export type ActionIconName = keyof typeof actionIconAssets;
