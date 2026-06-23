export interface ArtifactRow {
  id: string;
  artifactType: string;
  title: string;
  summary: string;
  sourceObjectId?: string;
  createdAt: string;
  extractorId?: string;
  extractorVersion?: string;
  confidence?: number;
  sourceAttribution?: string;
  attrs: Record<string, unknown>;
}

export interface FamilyCount {
  family: string;
  count: number;
}
