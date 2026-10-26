import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { McpSectionView } from '../components/McpSectionView';
import { McpHostToolsPanelContainer } from './McpHostToolsPanelContainer';

export function McpSectionContainer() {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);
  return <McpSectionView t={t} expanded={expanded} onToggle={() => setExpanded((value) => !value)}
    hostToolList={<McpHostToolsPanelContainer />} />;
}
