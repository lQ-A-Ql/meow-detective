export type FileIconColorName =
  | 'red'
  | 'green'
  | 'blue'
  | 'emerald'
  | 'yellow'
  | 'orange'
  | 'cyan'
  | 'neutral'
  | 'cobalt'
  | 'muted'
  | 'ocean'
  | 'amber'
  | 'violet'
  | 'purple'
  | 'teal'
  | 'slate'
  | 'soft'
  | 'faint'
  | 'dark'
  | 'default';

const FILE_ICON_COLOR_TOKENS: Record<FileIconColorName, string> = {
  red: '--forensics-file-icon-red',
  green: '--forensics-file-icon-green',
  blue: '--forensics-file-icon-blue',
  emerald: '--forensics-file-icon-emerald',
  yellow: '--forensics-file-icon-yellow',
  orange: '--forensics-file-icon-orange',
  cyan: '--forensics-file-icon-cyan',
  neutral: '--forensics-file-icon-neutral',
  cobalt: '--forensics-file-icon-cobalt',
  muted: '--forensics-file-icon-muted',
  ocean: '--forensics-file-icon-ocean',
  amber: '--forensics-file-icon-amber',
  violet: '--forensics-file-icon-violet',
  purple: '--forensics-file-icon-purple',
  teal: '--forensics-file-icon-teal',
  slate: '--forensics-file-icon-slate',
  soft: '--forensics-file-icon-soft',
  faint: '--forensics-file-icon-faint',
  dark: '--forensics-file-icon-dark',
  default: '--forensics-file-icon-default',
};

export function fileIconColor(name: FileIconColorName) {
  return `var(${FILE_ICON_COLOR_TOKENS[name]})`;
}
