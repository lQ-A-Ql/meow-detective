import { useTranslation } from 'react-i18next';
import { registryBrowserClient } from '@/lib/api/registry-browser';
import { useNativeRegistryContext } from '../hooks/use-native-registry-context';
import { RegistryBrowserContainer } from './RegistryBrowserContainer';

export function NativeRegistryBrowserContainer({ dataSourceId }: { dataSourceId?: string }) {
  const context = useNativeRegistryContext(dataSourceId);
  const { t } = useTranslation();
  return context ? <RegistryBrowserContainer client={registryBrowserClient} context={context} />
    : <p className="registry-browser-message">{t('registryBrowser.errors.sourceUnavailable')}</p>;
}
