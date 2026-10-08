export interface McpResourceListProps { resources: Array<{ uri: string; name: string; description?: string }>; loading: boolean; error?: string | null; onRefresh: () => void }
