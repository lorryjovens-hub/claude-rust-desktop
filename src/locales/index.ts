import en from './en.json';
import { resolveLanguage } from './languages';

export type TranslationTree = { [key: string]: string | TranslationTree };

// All dictionaries are bundled locally; changing language never needs a network request.
const dictionaries = import.meta.glob<TranslationTree>(['./*.json', '!./languages.json'], { eager: true, import: 'default' });

export function getTranslations(language: string): TranslationTree {
  return dictionaries[`./${resolveLanguage(language) || 'en'}.json`] || en;
}

function lookup(tree: TranslationTree, key: string): string | undefined {
  let value: string | TranslationTree | undefined = tree;
  for (const part of key.split('.')) {
    if (!value || typeof value === 'string') return undefined;
    value = value[part];
  }
  return typeof value === 'string' ? value : undefined;
}

export function translate(language: string, key: string, params?: Record<string, string | number>): string {
  const text = lookup(getTranslations(language), key) ?? lookup(en, key) ?? key;
  // A callback keeps replacement values such as "$&" literal and never evaluates keys as regexes.
  return text.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
    params && Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : placeholder,
  );
}
