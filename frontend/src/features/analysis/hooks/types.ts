import type { AnalysisExtractionPageRequest, DataSourceSummary } from '@/types/models';

export type AnalysisSource = Pick<DataSourceSummary, 'id' | 'platform'>;

export type OptionalAnalysisPageRequest = Omit<
  Partial<AnalysisExtractionPageRequest>,
  'dataSourceId'
> & {
  source?: AnalysisSource;
};
