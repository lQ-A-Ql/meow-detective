import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { i18nTranslations } from './resources';

i18n.use(initReactI18next).init({
  resources: {
    'zh-CN': { translation: i18nTranslations['zh-CN'] },
    en: { translation: i18nTranslations.en },
  },
  lng: 'zh-CN',
  fallbackLng: 'en',
  interpolation: {
    escapeValue: false,
  },
  react: {
    useSuspense: false,
  },
});

export default i18n;
