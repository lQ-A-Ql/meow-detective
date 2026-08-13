// Plugin analysis module contract — mirrors
// crates/transport/src/dto/analysis_plugin.rs (camelCase on the wire).

export interface PluginFamilyCount {
  family: string;
  count: number;
}

export interface PluginModule {
  pluginId: string;
  displayName: string;
  pluginVersion: string;
  evidencePlatform: string;
  families: PluginFamilyCount[];
  totalCount: number;
  warnings: string[];
}

export interface PluginArtifactEntry {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  title: string;
  summary: string;
  confidence?: number;
  attrs: Record<string, unknown>;
  createdAt: string;
}

export interface PluginFamilyEntries {
  pluginId: string;
  family: string;
  totalCount: number;
  truncated: boolean;
  entries: PluginArtifactEntry[];
}

export interface PluginFamilyEntriesRequest {
  dataSourceId: string;
  pluginId: string;
  family: string;
  offset: number;
  limit: number;
}
