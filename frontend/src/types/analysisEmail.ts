import type { AnalysisParseStatus } from './analysis';

export interface EmailExtractionSummary {
  status: AnalysisParseStatus;
  total: number;
  messages: EmailMessage[];
  generatedAt: string;
  warnings: string[];
}

export interface EmailAttachment {
  fileName: string;
  size?: number;
  mimeType?: string;
  contentId?: string;
}

export interface EmailHeader {
  name: string;
  value: string;
}

export interface EmailMessage {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  sentAt?: string;
  receivedAt?: string;
  from: string;
  to: string[];
  cc: string[];
  bcc: string[];
  replyTo?: string;
  returnPath?: string;
  subject: string;
  messageId: string;
  inReplyTo?: string;
  references: string[];
  attachments: string[];
  attachmentDetails: EmailAttachment[];
  headers: EmailHeader[];
  bodyPreview: string;
  bodyPlain?: string;
  bodyHtml?: string;
  xMailer?: string;
  xOriginatingIp?: string;
  containerPath?: string;
  messageClass?: string;
  attachmentCount: number;
  isDeleted?: boolean;
}
