import caseOverview from './watermarks/case-overview.webp';
import emulation from './structural/emulation-edge.webp';
import search from './watermarks/search.webp';
import reports from './structural/report-side.webp';
import settings from './structural/settings-side.webp';
import filePreview from './watermarks/file-preview.webp';
import moonCat from './ornaments/moon-cat.webp';
import paperButterflies from './ornaments/paper-butterflies.webp';
import inkBotanicals from './ornaments/ink-botanicals.webp';

/** Artwork ownership stays in the asset bank; views use semantic motif names. */
export const watermarkArtwork = {
  'case-overview': caseOverview,
  emulation,
  search,
  reports,
  settings,
  'file-preview': filePreview,
  'moon-cat': moonCat,
  'paper-butterflies': paperButterflies,
  'ink-botanicals': inkBotanicals,
} as const;

export type WatermarkArtwork = keyof typeof watermarkArtwork;
