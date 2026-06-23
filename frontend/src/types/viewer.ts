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
  lineNumber: number;
  language: string | null;
}

export interface ImagePreviewResponse {
  dataUrl: string;
  mimeType: string;
  width: number;
  height: number;
  size: number;
}
