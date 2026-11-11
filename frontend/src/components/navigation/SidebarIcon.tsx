import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import type { IconDefinition } from '@fortawesome/fontawesome-svg-core';
import {
  faChartLine,
  faChartPie,
  faClipboardCheck,
  faComputer,
  faCubes,
  faFileLines,
  faFolderTree,
  faGear,
  faHouse,
  faListCheck,
  faMagnifyingGlass,
  faNetworkWired,
  faTimeline,
} from '@fortawesome/free-solid-svg-icons';

const sidebarIcons = {
  home: faHouse,
  files: faFolderTree,
  emulation: faComputer,
  analysis: faChartLine,
  infrastructure: faNetworkWired,
  v2: faClipboardCheck,
  v3: faChartPie,
  search: faMagnifyingGlass,
  timeline: faTimeline,
  artifacts: faCubes,
  reports: faFileLines,
  settings: faGear,
  jobs: faListCheck,
} as const satisfies Record<string, IconDefinition>;

export type SidebarIconName = keyof typeof sidebarIcons;

export function SidebarIcon({ name, className = '' }: { name: SidebarIconName; className?: string }) {
  return <FontAwesomeIcon icon={sidebarIcons[name]} className={`shrink-0 ${className}`} />;
}
