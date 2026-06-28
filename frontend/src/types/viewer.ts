export interface ViewerHandle {
  handleId: string;
  size: number;
  mime?: string;
}

export interface ViewerRangeRequest {
  handleId: string;
  offset: number;
  length: number;
}

export interface ViewerRangeResponse {
  kind: 'hex' | 'text';
  lines: string[];
  encoding?: string;
  rawBytes?: number[];
}

export type HexViewerMode = 'full' | 'chunked';

export interface HexLoadedRange {
  start: number;
  end: number;
}

export interface HexByteWindowLines extends Array<string> {
  rawBytes?: number[];
  baseOffset?: number;
  fileSize?: number;
}

export interface FileHexViewerState {
  handle: ViewerHandle;
  mode: HexViewerMode;
  chunkSize: number;
  fileSize: number;
  lines: HexByteWindowLines;
  rawBytes: number[];
  baseOffset: number;
  loadedRanges: HexLoadedRange[];
  activeOffset: number;
  jumpOffsetInput: string;
  isFullyLoaded: boolean;
  isLoadingMore: boolean;
  hasMoreBefore: boolean;
  hasMoreAfter: boolean;
  error?: string;
}

export interface MediaUrl {
  url?: string;
  handleId?: string;
  mimeType: string;
  size: number;
  canReadRanges: boolean;
  mode?: 'inline' | 'protocol' | 'rangeFallback';
  previewMode?: 'inline' | 'protocol' | 'rangeFallback' | 'range';
  previewBytes?: number;
}

export interface MediaRangeRequest {
  handleId: string;
  offset: number;
  length: number;
}

export interface MediaRangeResponse {
  offset: number;
  bytesBase64: string;
  bytesRead: number;
  eof: boolean;
}

export interface TextPreviewResponse {
  content: string;
  encoding: string;
  isTruncated: boolean;
  isBinary: boolean;
  lineCount: number;
  language: string | null;
  hexDump?: string;
}

export interface ImagePreviewResponse {
  dataUrl: string;
  mimeType: string;
  width: number;
  height: number;
  size: number;
}
