import home from './home.svg';
import files from './files.svg';
import emulation from './emulation.svg';
import analysis from './analysis.svg';
import infrastructure from './infrastructure.svg';
import v2 from './v2.svg';
import v3 from './v3.svg';
import search from './search.svg';
import timeline from './timeline.svg';
import artifacts from './artifacts.svg';
import reports from './reports.svg';
import settings from './settings.svg';
import jobs from './jobs.svg';
import selectedHome from './selected/home.svg';
import selectedFiles from './selected/files.svg';
import selectedEmulation from './selected/emulation.svg';
import selectedAnalysis from './selected/analysis.svg';
import selectedInfrastructure from './selected/infrastructure.svg';
import selectedV2 from './selected/v2.svg';
import selectedV3 from './selected/v3.svg';
import selectedArtifacts from './selected/artifacts.svg';
import selectedReports from './selected/reports.svg';
import selectedSettings from './selected/settings.svg';

export const sidebarIconAssets = {
  home, files, emulation, analysis, infrastructure, v2, v3, search, timeline, artifacts, reports, settings, jobs,
} as const;

/** Inverse artwork uses opaque silhouettes with transparent detail cutouts.
 * Search and timeline retain their open line geometry in both states. */
export const sidebarSelectedIconAssets: Partial<Record<keyof typeof sidebarIconAssets, string>> = {
  home: selectedHome, files: selectedFiles, emulation: selectedEmulation,
  analysis: selectedAnalysis, infrastructure: selectedInfrastructure,
  v2: selectedV2, v3: selectedV3, artifacts: selectedArtifacts,
  reports: selectedReports, settings: selectedSettings,
};
