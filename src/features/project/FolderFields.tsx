import { useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { Folder, FolderPlus, X } from 'lucide-react';

export default function FolderFields({ folders, onChange, disabled = false, compact = false }: {
  folders: string[]; onChange: (paths: string[]) => void; disabled?: boolean; compact?: boolean;
}) {
  const { t } = useI18n();
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const [adding, setAdding] = useState(false);
  const add = (paths: string[]) => {
    onChange([...new Set([...folders, ...paths.map(p => p.trim()).filter(Boolean)])]);
    setPath('');
    setError('');
  };
  const browse = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, multiple: true, title: t('projectDialogs.addSourceFolders') });
      if (selected) add(Array.isArray(selected) ? selected : [selected]);
    } catch (error) { setError(String(error)); }
  };
  return <div className="space-y-3">
    <label className="block text-sm font-medium text-claude-text">{t('projectDialogs.sourceFolders')}</label>
    <div className="rounded-xl border border-claude-border bg-claude-input p-3 space-y-2">
      {folders.map(folder => <div key={folder} className="flex items-center gap-2 text-sm text-claude-text">
        <Folder size={15} className="shrink-0" />
        <span title={folder} className="flex-1 min-w-0 truncate">{compact ? folder.split(/[\\/]/).filter(Boolean).pop() || folder : folder}</span>
        <button type="button" disabled={disabled} aria-label={t('projectDialogs.removeFolder', { path: folder })} onClick={() => onChange(folders.filter(p => p !== folder))} className="p-1 rounded hover:bg-claude-hover disabled:opacity-40"><X size={14} /></button>
      </div>)}
      {!folders.length && <p className="py-3 text-center text-sm text-claude-textSecondary">{t('projectDialogs.needFolder')}</p>}
      {compact ? <button type="button" disabled={disabled} onClick={() => (window as any).__TAURI_INTERNALS__ ? browse() : setAdding(true)} className="flex w-full items-center gap-2 border-t border-claude-border pt-3 text-sm text-claude-text"><FolderPlus size={15} /> {t('projectDialogs.addFolder')}</button> : !!(window as any).__TAURI_INTERNALS__ && <button type="button" disabled={disabled} onClick={browse} className="mx-auto flex items-center gap-2 rounded-lg bg-claude-hover px-3 py-1.5 text-sm text-claude-text"><FolderPlus size={15} /> {t('projectDialogs.browseFolders')}</button>}
      {(!compact || adding) && <div className="flex gap-2 pt-2">
        <input aria-label={t('projectDialogs.folderPath')} placeholder={t('projectDialogs.absolutePath')} value={path} disabled={disabled} onChange={e => setPath(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); if (path.trim()) add([path]); } }} className="min-w-0 flex-1 rounded-lg border border-claude-border bg-transparent px-3 py-2 text-sm text-claude-text outline-none focus:border-blue-500" />
        <button type="button" disabled={disabled || !path.trim()} onClick={() => add([path])} className="rounded-lg bg-claude-hover px-3 text-sm text-claude-text disabled:opacity-40">{t('projectDialogs.add')}</button>
      </div>}
    </div>
    {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
  </div>;
}
