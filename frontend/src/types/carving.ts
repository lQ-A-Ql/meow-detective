export interface PngCarvingRequest { dataSourceId: string; offset?: string; length?: string }
export interface ZipCarvingRequest { dataSourceId: string; offset?: string; length?: string }
export interface CarvedArtifact { artifactId: string; dataSourceId: string; sourceRevision: string; view: string; offset: string; length: string; sha256: string; validatorVersion: string }
export interface CarvingRejection { offset: string; status: string; reason: string }
export interface PngCarvingSummary { scannedBytes: string; rangeOffset: string; rangeLength: string; candidates: number; complete: number; partial: number; rejected: number; artifacts: CarvedArtifact[]; artifactsTruncated: boolean; rejections: CarvingRejection[] }
export interface ZipCarvingSummary extends PngCarvingSummary {}
export interface CarvedArtifactPage { items: CarvedArtifact[]; nextCursor?: string }
export interface CarvedRange { artifactId: string; offset: string; base64: string; eof: boolean }
export interface CarvedExport { artifactId: string; bytesWritten: string; sha256: string }
