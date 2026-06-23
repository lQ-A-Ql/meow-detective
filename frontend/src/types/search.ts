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
  tookMs: number;
  items: SearchHit[];
}
