import languages from './languages.json';

export const supportedLanguages = languages;

/** Keep the existing en/zh storage IDs while accepting browser BCP 47 tags. */
export function resolveLanguage(value: string | null | undefined): string | undefined {
  if (!value) return undefined;
  const tag = value.trim().replaceAll('_', '-').toLowerCase();
  const exact = supportedLanguages.find(item => item.id.toLowerCase() === tag || item.locale.toLowerCase() === tag);
  if (exact) return exact.id;
  const parts = tag.split('-');
  const base = parts[0];
  if (base === 'zh') return parts.some(part => ['hant', 'tw', 'hk', 'mo'].includes(part)) ? 'zh-TW' : 'zh';
  if (base === 'es') return parts.some(part => /^([a-z]{2}|\d{3})$/.test(part) && part !== 'es') ? 'es-419' : 'es-ES';
  return supportedLanguages.find(item => item.locale.split('-')[0] === base)?.id;
}

export function getLanguage(value: string) {
  const id = resolveLanguage(value) || 'en';
  return supportedLanguages.find(item => item.id === id)!;
}

export function getInitialLanguage(): string {
  const saved = resolveLanguage(localStorage.getItem('app_language'));
  if (saved) return saved;
  for (const tag of [...(navigator.languages || []), navigator.language]) {
    const language = resolveLanguage(tag);
    if (language) return language;
  }
  return 'en';
}

export function applyDocumentLanguage(value: string) {
  const language = getLanguage(value);
  document.documentElement.lang = language.locale;
  document.documentElement.dir = language.direction;
}

export function getLanguageDisplayName(value: string, displayLanguage: string): string {
  const language = getLanguage(value);
  try {
    return new Intl.DisplayNames([getLanguage(displayLanguage).locale], { type: 'language' }).of(language.locale) || language.nativeName;
  } catch {
    return language.nativeName;
  }
}
