export interface StixExportRequest {
  artifactTypeFilter?: string;
}

export interface StixExportResult {
  json: string;
  objectCount: number;
  indicatorCount: number;
  observedDataCount: number;
  relationshipCount: number;
  generatedAt: string;
}
