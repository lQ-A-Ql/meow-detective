import { useTranslation } from 'react-i18next';
import { useNativeRegistryContext } from '@/features/registry-browser/hooks/use-native-registry-context';
import { RegistryHardwareContainer } from './RegistryHardwareContainer';

export function NativeRegistryHardwareContainer({ dataSourceId }: { dataSourceId: string }) {
  const context = useNativeRegistryContext(dataSourceId);
  const { t } = useTranslation();
  return context ? <RegistryHardwareContainer key={`${context.caseId}:${context.dataSourceId}:${context.sourceRevision}`} context={context} />
    : <p className="registry-browser-message">{t('registryBrowser.errors.sourceUnavailable')}</p>;
}
