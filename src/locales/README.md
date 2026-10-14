# Interface translations

`languages.json` is the supported-language registry. It holds stable storage IDs,
BCP 47 tags, native display names and text direction. Keep `en` and `zh` IDs for
compatibility with existing preferences. Taiwan Traditional Chinese uses `zh-TW`;
Spain and Latin American Spanish use separate `es-ES` and `es-419` dictionaries.

Each supported language has a JSON dictionary with the same keys as `en.json`.
`index.ts` bundles the dictionaries locally; there are no runtime translation
requests. Components should use `useI18n().t(key, params)` and never store a
translated label as an action ID. `locale` from the same hook is suitable for
`Intl` date and number formatting. Missing keys fall back to English. User-entered
names, messages, paths and code are not translated.

To add a language:

1. Add its metadata to `languages.json` and a dictionary named after its `id`.
2. Translate every English key, keeping `{placeholder}` names and markup intact.
3. Verify switching, persistence, keyboard navigation and text direction.
4. Run `npm run test:run` and `npm run build`.

The additional dictionaries started as machine translations and have targeted
edits for core interface terminology. They have not received a complete native
speaker review. Existing components with hard-coded strings still need migration
to `t()`; dictionary coverage does not imply every application screen is localized.

`python scripts/seed-translations.py [locale-id ...]` is an optional maintainer
tool. It sends **public interface dictionary text** to Google's translation
endpoint, protects placeholders, and caches intermediate results in the OS temp
directory. It is not part of installation, builds or the running app. It overwrites
the requested dictionaries, so review the resulting diff. Keep editorial
corrections in `scripts/translation-overrides.json`; the seeding tool reapplies
these on every run. Do not put private content or credentials in dictionaries.
