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

/** One self-described plugin action from `list_plugin_actions`. */
export interface PluginActionDescriptor {
  id: string;
  label: string;
  description?: string;
  /** Currently "file" | "none". */
  inputKind: string;
}

/**
 * A page-1-verified key returned for explicit local investigator display.
 */
export interface WeChatRecoveredKey {
  databaseName: string;
  keyHex: string;
}

/** Outcome of a WeChat database-key recovery run (`recover_wechat_keys`). */
export interface WeChatKeyRecoveryResult {
  candidatesSeen: number;
  recoveredCount: number;
  matchedDbNames: string[];
  unmatchedDbNames: string[];
  recoveredKeys: WeChatRecoveredKey[];
}
