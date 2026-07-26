export interface SearchSnippet {
  text: string;
  highlights: Array<{ start: number; end: number }>;
}

export interface SearchHit {
  fileId: string;
  path: string;
  score: number;
  snippets: SearchSnippet[];
}

export interface SearchResultPage {
  total: number;
  available: number;
  truncated: boolean;
  tookMs: number;
  items: SearchHit[];
  nextCursor?: string;
}
