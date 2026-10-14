import React, { useEffect, useRef, useState } from 'react';
import { Sun, Moon, Monitor, X } from 'lucide-react';
import { useI18n } from '../hooks/useI18n';
import { openGeneralConfiguration, useGeneralSettingsStore } from '../stores/useGeneralSettingsStore';
import PermissionModeSelector from './PermissionModeSelector';
import LanguagePicker from './LanguagePicker';
import { getErrorMessage } from '../utils/errorHelpers';

export default function GeneralPreferences({ chatFont }: { chatFont?: React.ReactNode }) {
  const { t } = useI18n();
  const settings = useGeneralSettingsStore();
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [openError, setOpenError] = useState('');
  const [opening, setOpening] = useState(false);
  const [fontInput, setFontInput] = useState(String(settings.fontSize));
  const dialogRef = useRef<HTMLDivElement>(null);
  useEffect(() => { setFontInput(String(settings.fontSize)); }, [settings.fontSize]);
  useEffect(() => {
    if (!shortcutsOpen) return;
    const previous = document.activeElement as HTMLElement | null;
    const handleTab = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const controls = dialogRef.current?.querySelectorAll<HTMLElement>('button, select');
      if (!controls?.length) return;
      const first = controls[0], last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    };
    document.addEventListener('keydown', handleTab);
    return () => { document.removeEventListener('keydown', handleTab); previous?.focus(); };
  }, [shortcutsOpen]);
  const control = 'rounded-xl bg-claude-hover px-4 py-3 text-[15px] text-claude-text outline-none focus-visible:ring-2 focus-visible:ring-blue-500';
  const row = (name: string, description: string | null, child: React.ReactNode) => (
    <div className="flex items-center justify-between gap-6 border-b border-claude-border py-6">
      <div className="min-w-0"><div className="text-[16px] text-claude-text">{t(`general.${name}`)}</div>
        {description && <p className="mt-2 text-[14px] text-claude-textSecondary">{t(`general.${description}`)}</p>}
      </div>
      <div className="shrink-0">{child}</div>
    </div>
  );
  return (
    <section>
      <div className="flex justify-end">
        <button disabled={opening} className="rounded-lg border border-claude-border px-3 py-2 text-[13px] hover:bg-claude-hover disabled:opacity-50" onClick={async () => {
          setOpening(true); setOpenError('');
          try { await openGeneralConfiguration(); } catch (error) { setOpenError(getErrorMessage(error)); }
          finally { setOpening(false); }
        }}>{t('general.openConfiguration')}</button>
      </div>
      {(openError || settings.saveError) && <p role="alert" className="mt-2 text-sm text-red-500">{openError || settings.saveError}</p>}
      {row('permission', 'permissionDescription', <PermissionModeSelector value={settings.defaultPermissionMode} onChange={mode => settings.update({ defaultPermissionMode: mode })} className="general-permission-selector" />)}
      {row('language', null, <LanguagePicker />)}
      <div className="border-b border-claude-border py-6">
        <div className="mb-4 text-[16px]">{t('settings.appearance')}</div>
        <div className="grid grid-cols-3 gap-3">
          {([{ value: 'light', icon: Sun }, { value: 'dark', icon: Moon }, { value: 'system', icon: Monitor }] as const).map(({ value, icon: Icon }) => (
            <button key={value} aria-pressed={settings.theme === value} onClick={() => settings.update({ theme: value })}
              className={`flex flex-col items-center gap-3 rounded-2xl border py-6 text-[16px] hover:bg-claude-hover ${settings.theme === value ? 'border-claude-textSecondary bg-claude-hover' : 'border-claude-border'}`}>
              <Icon size={21} /><span>{t(`general.${value}`)}</span>
            </button>
          ))}
        </div>
      </div>
      {row('fontSize', 'fontSizeDescription', <div className="flex items-center gap-2"><input aria-label={t('general.fontSize')} type="number" min={10} max={32} value={fontInput} onChange={event => {
        setFontInput(event.target.value);
        const size = Number(event.target.value);
        if (size >= 10 && size <= 32) settings.update({ fontSize: size });
      }} onBlur={() => {
        settings.update({ fontSize: fontInput === '' ? settings.fontSize : Number(fontInput) });
        setFontInput(String(useGeneralSettingsStore.getState().fontSize));
      }} className={`${control} w-20 text-center`} /><span className="text-claude-textSecondary">px</span></div>)}
      {chatFont}
      <fieldset className="min-w-0 border-b border-claude-border py-6">
        <legend className="float-left w-full text-[16px] text-claude-text">{t('general.workDetails')}</legend>
        <p className="clear-both pt-2 text-[14px] text-claude-textSecondary">{t('general.workDetailsDescription')}</p>
        <div className="mt-4 grid gap-3 sm:grid-cols-2">
          {(['chat', 'work', 'study', 'code'] as const).map(mode => <label key={mode} className="flex items-center justify-between gap-3 rounded-xl border border-claude-border p-3">
            <span className="text-[15px]">{mode[0].toUpperCase() + mode.slice(1)}</span>
            <select aria-label={`${mode[0].toUpperCase() + mode.slice(1)} · ${t('general.workDetails')}`} className={control} value={settings.workDetailsByMode[mode]} onChange={event => settings.update({ workDetailsByMode: { ...settings.workDetailsByMode, [mode]: event.target.value as typeof settings.workDetailsByMode[typeof mode] } })}>
              {['compact', 'standard', 'detailed', 'verbose'].map(value => <option key={value} value={value}>{t(`general.${value}`)}</option>)}
            </select>
          </label>)}
        </div>
      </fieldset>
      {row('codingView', 'codingViewDescription', <button role="switch" aria-label={t('general.codingView')} aria-checked={settings.showCodingView} onClick={() => settings.update({ showCodingView: !settings.showCodingView })}
        className={`relative h-6 w-11 rounded-full transition-colors ${settings.showCodingView ? 'bg-claude-text' : 'bg-claude-border'}`}><span className={`absolute top-0.5 h-5 w-5 rounded-full bg-claude-bg shadow transition-transform ${settings.showCodingView ? 'left-0.5 translate-x-5' : 'left-0.5'}`} /></button>)}
      <fieldset className="min-w-0 border-b border-claude-border py-6">
        <legend className="float-left w-full text-[16px] text-claude-text">{t('general.showUsage')}</legend>
        <p className="clear-both pt-2 text-[14px] text-claude-textSecondary">{t('general.showUsageDescription')}</p>
        <div className="mt-4 grid gap-3 sm:grid-cols-2">
          {(['chat', 'work', 'study', 'code'] as const).map(mode => <div key={mode} className="flex items-center justify-between gap-3 rounded-xl border border-claude-border p-3">
            <span className="text-[15px]">{mode[0].toUpperCase() + mode.slice(1)}</span>
            <button type="button" role="switch" aria-label={`${mode[0].toUpperCase() + mode.slice(1)} · ${t('general.showUsage')}`} aria-checked={settings.showUsageByMode[mode]} onClick={() => settings.update({ showUsageByMode: { ...settings.showUsageByMode, [mode]: !settings.showUsageByMode[mode] } })}
              className={`relative h-6 w-11 rounded-full transition-colors ${settings.showUsageByMode[mode] ? 'bg-claude-text' : 'bg-claude-border'}`}><span className={`absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-claude-bg shadow transition-transform ${settings.showUsageByMode[mode] ? 'translate-x-5' : ''}`} /></button>
          </div>)}
        </div>
      </fieldset>
      {row('showAiDisclaimer', 'showAiDisclaimerDescription', <button type="button" role="switch" aria-label={t('general.showAiDisclaimer')} aria-checked={settings.showAiDisclaimer} onClick={() => settings.update({ showAiDisclaimer: !settings.showAiDisclaimer })}
        className={`relative h-6 w-11 rounded-full transition-colors ${settings.showAiDisclaimer ? 'bg-claude-text' : 'bg-claude-border'}`}><span className={`absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-claude-bg shadow transition-transform ${settings.showAiDisclaimer ? 'translate-x-5' : ''}`} /></button>)}
      {row('shortcuts', 'shortcutsDescription', <button className={control} onClick={() => setShortcutsOpen(true)}>{t('general.editShortcuts')}</button>)}
      {row('chatLinks', 'chatLinksDescription', <select aria-label={t('general.chatLinks')} className={control} value={settings.chatLinks} onChange={event => settings.update({ chatLinks: event.target.value as typeof settings.chatLinks })}>
        <option value="sidebar">{t('general.sidebar')}</option><option value="browser">{t('general.browser')}</option>
      </select>)}
      {row('busySend', 'busySendDescription', <select aria-label={t('general.busySend')} className={control} value={settings.busySendBehavior} onChange={event => settings.update({ busySendBehavior: event.target.value as typeof settings.busySendBehavior })}>
        <option value="queue">Queue</option><option value="steer">Steer</option>
      </select>)}
      {shortcutsOpen && <div className="fixed inset-0 z-[110] flex items-center justify-center bg-black/40 p-6" onClick={() => setShortcutsOpen(false)} onKeyDown={event => { if (event.key === 'Escape') setShortcutsOpen(false); }}>
        <div ref={dialogRef} role="dialog" aria-modal="true" aria-label={t('general.shortcuts')} className="w-full max-w-md rounded-2xl border border-claude-border bg-claude-bg p-6 shadow-xl" onClick={event => event.stopPropagation()}>
          <div className="mb-6 flex items-center justify-between"><h3>{t('general.shortcuts')}</h3><button autoFocus aria-label={t('common.close')} onClick={() => setShortcutsOpen(false)}><X size={20} /></button></div>
          {(['sendKey', 'newlineKey'] as const).map(key => <label key={key} className="mb-4 flex items-center justify-between gap-4">
            <span>{t(key === 'sendKey' ? 'settings.sendMessageLabel' : 'settings.newlineLabel')}</span>
            <select className={control} value={settings[key]} onChange={event => settings.update({ [key]: event.target.value,
              ...(key === 'sendKey' && event.target.value === settings.newlineKey ? { newlineKey: event.target.value === 'enter' ? 'shift_enter' : 'enter' } : {}),
            })}>
              {(key === 'sendKey' ? ['enter', 'ctrl_enter', 'cmd_enter', 'alt_enter'] : ['enter', 'shift_enter', 'ctrl_enter', 'cmd_enter', 'alt_enter']).filter(value => key !== 'newlineKey' || value !== settings.sendKey).map(value => <option key={value} value={value}>{value.split('_').map(word => word[0].toUpperCase() + word.slice(1)).join('+')}</option>)}
            </select>
          </label>)}
          <p className="mt-4 text-[13px] text-claude-textSecondary">{t('general.fixedShortcuts')}</p>
        </div>
      </div>}
    </section>
  );
}
