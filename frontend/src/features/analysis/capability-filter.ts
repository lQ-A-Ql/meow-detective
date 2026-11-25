import { isSourceCapabilityEligible } from '@/features/capability/gating';
import type { CapabilityMatrix, DataSourceSummary } from '@/types/models';

const SUPPORTED_PLATFORMS = new Set(['windows', 'linux', 'android']);

/**
 * Sources eligible for the analysis workspace. Catalog readiness (matrix row
 * ready/partial) admits the source; platformFacts/artifacts rows are advisory
 * and never block entry, so running/partial extraction keeps existing results
 * viewable.
 */
export function selectAnalysisCandidates(
  dataSources: DataSourceSummary[] | undefined,
  matrix: CapabilityMatrix | undefined,
): DataSourceSummary[] {
  return (dataSources ?? []).filter((source) => {
    if (!SUPPORTED_PLATFORMS.has(source.platform)) {
      return false;
    }
    return isSourceCapabilityEligible(matrix, source.id, 'catalog');
  });
}
