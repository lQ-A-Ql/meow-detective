import type {
  BrowserHistorySummary,
  EmailExtractionSummary,
  EvtxEventSummary,
  RegistryExtractionSummary,
} from '@/types/models';
import type { AnalysisTabKey } from './types';

export function windowsNodeCounts(
  registry?: RegistryExtractionSummary,
  browser?: BrowserHistorySummary,
  email?: EmailExtractionSummary,
  events?: EvtxEventSummary,
): Partial<Record<AnalysisTabKey, number>> {
  const counts: Partial<Record<AnalysisTabKey, number>> = {};
  if (registry) counts.registry = registry.total;
  if (browser) {
    counts.browser =
      browser.visitTotal +
      browser.downloadTotal +
      browser.cookieTotal +
      browser.sessionTotal +
      browser.passwordTotal +
      browser.extensionTotal;
  }
  if (email) counts.email = email.total;
  if (events) counts.eventlogs = events.totalCount;
  return counts;
}
