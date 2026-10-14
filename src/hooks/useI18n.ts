import { useCallback, useMemo } from 'react';
import { useUIStore } from '../stores/useUIStore';
import { translate } from '../locales';
import { getLanguage } from '../locales/languages';

export function useI18n() {
  const language = useUIStore((state) => state.language);

  const t = useCallback((key: string, params?: Record<string, string | number>): string => {
    return translate(language, key, params);
  }, [language]);

  const setLanguage = useCallback((lang: string) => {
    useUIStore.getState().setLanguage(lang);
  }, []);

  return useMemo(() => ({ t, language, locale: getLanguage(language).locale, setLanguage }), [t, language, setLanguage]);
}
