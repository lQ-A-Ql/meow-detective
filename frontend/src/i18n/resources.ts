import zhCN from './zh-CN.json';
import en from './en.json';
import registryBrowserZh from './registry-browser/zh-CN.json';
import registryBrowserEn from './registry-browser/en.json';

/** One composition point for centrally owned, independently maintained copy modules. */
export const i18nTranslations = {
  'zh-CN': { ...zhCN, registryBrowser: registryBrowserZh },
  en: { ...en, registryBrowser: registryBrowserEn },
};
