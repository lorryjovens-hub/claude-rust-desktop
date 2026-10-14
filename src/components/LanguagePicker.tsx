import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown, Search, X } from 'lucide-react';
import { useI18n } from '../hooks/useI18n';
import { getLanguage, getLanguageDisplayName, supportedLanguages } from '../locales/languages';
import { useDialogFocus } from '../features/project/useDialogFocus';

export default function LanguagePicker() {
  const { t, language, setLanguage } = useI18n();
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const id = useId();
  const dialogRef = useDialogFocus(open);
  const searchRef = useRef<HTMLInputElement>(null);
  useEffect(() => { if (open) searchRef.current?.focus(); }, [open]);
  const current = getLanguage(language);
  const options = useMemo(() => supportedLanguages.map(item => ({
    ...item,
    displayName: getLanguageDisplayName(item.id, language),
    englishName: getLanguageDisplayName(item.id, 'en'),
  })), [language]);
  const query = search.trim().toLocaleLowerCase();
  const visibleOptions = options.filter(item =>
    [item.nativeName, item.displayName, item.englishName, item.locale].some(value => value.toLocaleLowerCase().includes(query)),
  );
  const close = () => setOpen(false);

  return (
    <>
      <label id={`${id}-label`} className="block text-[13px] font-medium text-claude-textSecondary mb-1.5">
        {t('settings.languageLabel')}
      </label>
      <button
        type="button"
        aria-labelledby={`${id}-label ${id}-value`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? `${id}-dialog` : undefined}
        onClick={() => { setSearch(''); setOpen(true); }}
        className="flex w-full max-w-xs items-center justify-between gap-3 rounded-md border border-claude-border bg-claude-input px-3 py-2 text-start text-[14px] text-claude-text transition-colors hover:bg-claude-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
      >
        <span id={`${id}-value`} lang={current.locale} dir={current.direction}>{current.nativeName}</span>
        <ChevronDown aria-hidden="true" size={16} className="shrink-0 text-claude-textSecondary" />
      </button>
      {open && createPortal(
        <div
          className="fixed inset-0 z-[300] flex items-center justify-center bg-black/40 p-4"
          onMouseDown={event => { if (event.target === event.currentTarget) close(); }}
          onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } }}
        >
          <form
            ref={dialogRef}
            id={`${id}-dialog`}
            role="dialog"
            aria-modal="true"
            aria-labelledby={`${id}-title`}
            onSubmit={event => event.preventDefault()}
            className="flex max-h-[85vh] w-full max-w-5xl flex-col rounded-2xl border border-claude-border bg-claude-input p-5 text-claude-text shadow-2xl sm:p-8"
          >
            <div className="mb-6 flex items-start justify-between gap-4">
              <h2 id={`${id}-title`} className="text-2xl font-semibold sm:text-[28px]">{t('languagePicker.title')}</h2>
              <button type="button" aria-label={t('common.close')} onClick={close} className="rounded-md p-1 hover:bg-claude-hover focus-visible:ring-2 focus-visible:ring-blue-500">
                <X aria-hidden="true" size={22} />
              </button>
            </div>
            <div className="mb-4 flex items-center gap-2 rounded-lg border border-claude-border px-3 py-2 focus-within:border-blue-500">
              <Search aria-hidden="true" size={17} className="text-claude-textSecondary" />
              <input ref={searchRef} type="search" value={search} onChange={event => setSearch(event.target.value)} aria-label={t('languagePicker.search')} placeholder={t('languagePicker.search')} className="min-w-0 flex-1 bg-transparent text-sm outline-none" />
            </div>
            <div role="group" aria-label={t('languagePicker.title')} className="grid min-h-0 grid-cols-1 gap-2 overflow-y-auto sm:grid-cols-2 lg:grid-cols-3">
              {visibleOptions.map(item => (
                <button
                  key={item.id}
                  type="button"
                  aria-pressed={language === item.id}
                  onClick={() => { setLanguage(item.id); close(); }}
                  className={`flex items-start justify-between gap-3 rounded-xl px-4 py-3 text-start focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 ${language === item.id ? 'bg-blue-500/10' : 'hover:bg-claude-hover'}`}
                >
                  <span className="min-w-0 break-words">
                    <span lang={item.locale} dir={item.direction} className="block text-[16px] font-medium">{item.nativeName}</span>
                    <span className="mt-0.5 block text-[14px] text-claude-textSecondary">{item.displayName}</span>
                  </span>
                  {language === item.id && <Check aria-hidden="true" size={18} className="mt-1 shrink-0 text-blue-600 dark:text-blue-400" />}
                </button>
              ))}
              {!visibleOptions.length && <p role="status" className="col-span-full py-8 text-center text-sm text-claude-textSecondary">{t('languagePicker.noResults')}</p>}
            </div>
          </form>
        </div>,
        document.body,
      )}
    </>
  );
}
