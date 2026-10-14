import { afterEach, describe, expect, it, vi } from 'vitest';
import { getInitialLanguage, getLanguageDisplayName, resolveLanguage, supportedLanguages } from './languages';
import { getTranslations, translate, type TranslationTree } from './index';
import en from './en.json';

afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); });

function flatten(tree: TranslationTree, prefix = ''): Record<string, string> {
  return Object.fromEntries(Object.entries(tree).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof value === 'string' ? [[path, value]] : Object.entries(flatten(value, path));
  }));
}
const source = flatten(en);

describe('supported interface languages', () => {
  it.each([
    ['en-GB', 'en'], ['zh-CN', 'zh'], ['zh-Hans', 'zh'], ['zh_Hant_TW', 'zh-TW'],
    ['zh-HK', 'zh-TW'], ['zh-MO', 'zh-TW'], ['ja', 'ja-JP'], ['ko', 'ko-KR'],
    ['de-AT', 'de-DE'], ['fr-CA', 'fr-FR'], ['es-MX', 'es-419'], ['es-AR', 'es-419'],
    ['es', 'es-ES'], ['es-ES', 'es-ES'], ['pt-PT', 'pt-BR'], ['ar-EG', 'ar-SA'],
  ])('resolves %s to %s', (input, expected) => expect(resolveLanguage(input)).toBe(expected));

  it('uses a saved preference before browser languages and handles invalid storage', () => {
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['xx-YY', 'ja-JP', 'en-US']);
    localStorage.setItem('app_language', 'zh-TW');
    expect(getInitialLanguage()).toBe('zh-TW');
    localStorage.setItem('app_language', 'invalid');
    expect(getInitialLanguage()).toBe('ja-JP');
  });

  it('uses English when no saved or browser locale is supported', () => {
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['xx']);
    vi.spyOn(navigator, 'language', 'get').mockReturnValue('xx');
    expect(getInitialLanguage()).toBe('en');
  });

  it.each(supportedLanguages)('$id has its own complete dictionary with intact placeholders', language => {
    const dictionary = getTranslations(language.id);
    if (language.id !== 'en') expect(dictionary).not.toEqual(getTranslations('en'));
    const actual = flatten(dictionary);
    expect(Object.keys(source).filter(key => !(key in actual))).toEqual([]);
    for (const [key, value] of Object.entries(source)) {
      expect(actual[key].match(/\{\w+\}/g)?.sort() || [], `${language.id}.${key}`).toEqual(value.match(/\{\w+\}/g)?.sort() || []);
      expect(actual[key], `${language.id}.${key}`).not.toMatch(/__(?:TOKEN\d+|9\d{5})__|\[\d{5}\]/);
      if (value) expect(actual[key].trim(), `${language.id}.${key}`).not.toBe('');
    }
    expect(getLanguageDisplayName(language.id, 'en')).not.toBe(language.locale);
  });

  it('falls back safely and preserves literal parameters and intentionally empty strings', () => {
    expect(translate('not-a-language', 'common.save')).toBe('Save');
    expect(translate('en', 'missing.key')).toBe('missing.key');
    expect(translate('en', 'settings.day')).toBe('');
    expect(translate('en', 'common')).toBe('common');
    expect(translate('en', 'sidebar.chatOptions', { name: '$& {name}' })).toBe('Chat options for $& {name}');
  });
});
