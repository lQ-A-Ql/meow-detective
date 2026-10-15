import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { EmailExtractionSummary, EmailMessage } from '@/types/models';
export function useEmailExtractionModel(summary?: EmailExtractionSummary) {
  const { t } = useTranslation();
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [showHeaders, setShowHeaders] = useState(false);
  const info = summary ?? { status: 'unavailable' as const, total: 0, messages: [], generatedAt: '', warnings: [t('analysisPanels.email.unavailable')] };
  const selectedMessage = info.messages.find((msg) => msg.artifactId === selectedArtifactId);
  const handleRowClick = useCallback((row: EmailMessage) => setSelectedArtifactId((current) => row.artifactId === current ? null : row.artifactId), []);
  return { info, selectedArtifactId, selectedMessage, showHeaders, handleRowClick, toggleHeaders: () => setShowHeaders((current) => !current), attachmentCount: info.messages.filter((item) => item.attachmentCount > 0).length };
}
