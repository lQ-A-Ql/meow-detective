import type { AnalysisParseStatus } from './analysis';

export interface EmailExtractionSummary {
  status: AnalysisParseStatus;
  total: number;
  messages: EmailMessage[];
  accounts?: EmailAccount[];
  generatedAt: string;
  warnings: string[];
}

export interface EmailAccount {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  account?: string;
  email?: string;
  incomingServer?: string;
  outgoingServer?: string;
  incomingPort?: number;
  outgoingPort?: number;
  incomingSsl?: boolean;
  outgoingSsl?: boolean;
  password?: string;
  passwordStatus?: string;
  passwordAlgorithm?: string;
  mailClient: string;
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
  mailClient?: string;
  xOriginatingIp?: string;
  containerPath?: string;
  messageIndex?: number;
  messageClass?: string;
  contentStatus?: string;
  attachmentCount: number;
  isDeleted?: boolean;
}
