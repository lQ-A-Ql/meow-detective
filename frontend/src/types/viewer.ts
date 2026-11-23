export interface ViewerHandle {
  handleId: string;
  size: number;
  mime?: string;
  previewKind?: 'image' | 'video' | 'audio' | 'document' | 'imageMetadataOnly';
}

export interface ViewerRangeRequest {
  handleId: string;
  offset: number;
  length: number;
}

export interface ViewerRangeResponse {
  offset?: number;
  bytesRead?: number;
  eof?: boolean;
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
  mode: 'inline' | 'protocol' | 'rangeFallback';
  url?: string;
  handleId?: string;
  mimeType: string;
  size: number;
  canReadRanges: boolean;
}

export interface MediaPreview extends MediaUrl {
  previewMode: 'inline' | 'protocol' | 'rangeFallback' | 'range';
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

export type ImageMetadataStatus = 'present' | 'absent' | 'unsupported' | 'corrupt' | 'oversized';
export type ImageIssueSeverity = 'info' | 'warning' | 'error';

export interface ImageMetadataWarning {
  code: string;
  severity: ImageIssueSeverity;
  message: string;
  offset?: number;
}

export interface ImageExifBlock {
  source: string;
  offset: number;
  length: number;
  decoded: boolean;
  fieldCount: number;
  warnings?: ImageMetadataWarning[];
}

export interface ImageSegment {
  kind: string;
  offset: number;
  length: number;
  detail?: string;
}

export interface ImageStructureFinding {
  code: string;
  severity: ImageIssueSeverity;
  offset?: number;
  detail?: string;
}

export interface ImageStructureHashes {
  wholeFileSha256?: string;
  trailingSha256?: string;
  trailingHashedBytes?: number;
}

export interface ImageStructure {
  declaredEnd?: number;
  trailingBytes?: number;
  truncated: boolean;
  segmentCount: number;
  segments?: ImageSegment[];
  segmentsTruncated: boolean;
  findings?: ImageStructureFinding[];
  hashes: ImageStructureHashes;
}

export interface ImageMetadata {
  status: ImageMetadataStatus;
  format?: string;
  width?: number;
  height?: number;
  orientation?: number;
  make?: string;
  model?: string;
  software?: string;
  dateTimeOriginal?: string;
  createDate?: string;
  modifyDate?: string;
  lensModel?: string;
  latitude?: number;
  longitude?: number;
  altitude?: number;
  gpsDateTime?: string;
  warnings?: ImageMetadataWarning[];
  exifBlocks?: ImageExifBlock[];
  structure?: ImageStructure;
}

export type ImageStegStatus = 'completed' | 'unsupported' | 'oversized' | 'corrupt';
export type ImageStegBand = 'low' | 'medium' | 'high' | 'inconclusive';
export type ImageStegMethodKind = 'chiSquare' | 'rs' | 'spa';

export interface ImageStegMethod {
  method: ImageStegMethodKind;
  /** Detector estimate expressed as a ratio from 0 to 1. */
  estimate?: number;
  /** Suspicion score in points from 0 to 100. */
  score?: number;
  samples: number;
}

export interface ImageStegAssessment {
  /** Suspicion score in points from 0 to 100. */
  score?: number;
  band: ImageStegBand;
  methodsUsed: number;
  methods?: ImageStegMethod[];
}

export interface ImageStegPlane {
  plane: string;
  samples: number;
  uniqueValues: number;
  assessment?: ImageStegAssessment;
}

export interface ImageStegBlock {
  x: number;
  y: number;
  width: number;
  height: number;
  plane: string;
  /** Suspicion score in points from 0 to 100. */
  score: number;
  band: ImageStegBand;
  methodsUsed: number;
}

export interface ImageStegWarning {
  code: string;
  severity: ImageIssueSeverity;
  message: string;
}

export interface ImageStegReport {
  status: ImageStegStatus;
  format?: string;
  width?: number;
  height?: number;
  reason?: string;
  global?: ImageStegAssessment;
  dominantPlane?: string;
  planes?: ImageStegPlane[];
  blocks?: ImageStegBlock[];
  blocksScored: number;
  blocksTotal: number;
  warnings?: ImageStegWarning[];
}

export interface DocumentTable {
  columns: string[];
  rows: string[][];
}

export interface DocumentSection {
  title: string;
  lines: string[];
  table?: DocumentTable;
}

export interface DocumentPreviewResponse {
  kind: string;
  summary: string;
  sections: DocumentSection[];
  truncated: boolean;
  warnings?: string[];
}
