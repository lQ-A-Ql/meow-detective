import type { EmailExtractionSummary } from '@/types/models';
import { useEmailExtractionModel } from '../use-email-extraction-model';
import { EmailExtractionPanelView } from '../components/panels/EmailExtractionPanelView';
export function EmailExtractionPanel({ summary }: { summary?: EmailExtractionSummary }) { return <EmailExtractionPanelView model={useEmailExtractionModel(summary)} />; }
