export interface TextViewerProps {
  content: string;
  encoding: string;
  extension?: string;
  searchQuery?: string;
  isTruncated?: boolean;
  documentKey?: string;
}

export interface TextPageBoundary {
  startOffset: number;
  firstLineNumber: number;
}

export interface TextDocumentIndex {
  pages: TextPageBoundary[];
  totalLines: number;
}

/** Offsets refer to UTF-16 positions in the decoded preview, never evidence byte offsets. */
export interface TextPageLine {
  content: string;
  startOffset: number;
  endOffset: number;
  logicalStartOffset: number;
  lineNumber: number;
  continuation: boolean;
}

export interface TextPage {
  lines: TextPageLine[];
  startLine: number;
  endLine: number;
}

export interface TextSearchIndex {
  starts: Uint32Array;
  ends: Uint32Array;
}

export interface TextScrollRequest {
  kind: 'top' | 'match' | 'keep';
  startOffset?: number;
  matchIndex?: number;
}
