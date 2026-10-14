import { useState } from 'react';
import { Folder, FolderPlus, X } from 'lucide-react';

export default function FolderFields({ folders, onChange, disabled = false }: {
  folders: string[]; onChange: (paths: string[]) => void; disabled?: boolean;
}) {
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const add = (paths: string[]) => {
    onChange([...new Set([...folders, ...paths.map(p => p.trim()).filter(Boolean)])]);
    setPath('');
    setError('');
  };
  const browse = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, multiple: true, title: 'Add source folders' });
      if (selected) add(Array.isArray(selected) ? selected : [selected]);
    } catch (error) { setError(String(error)); }
  };
  return <div className="space-y-3">
    <label className="block text-sm font-medium text-claude-text">Source folders</label>
    <div className="rounded-xl border border-claude-border bg-claude-input p-3 space-y-2">
      {folders.map(folder => <div key={folder} className="flex items-center gap-2 text-sm text-claude-text">
        <Folder size={15} className="shrink-0" />
        <span className="flex-1 min-w-0 break-all">{folder}</span>
        <button type="button" disabled={disabled} aria-label={`Remove ${folder}`} onClick={() => onChange(folders.filter(p => p !== folder))} className="p-1 rounded hover:bg-claude-hover disabled:opacity-40"><X size={14} /></button>
      </div>)}
      {!folders.length && <p className="py-3 text-center text-sm text-claude-textSecondary">Add at least one folder on this computer</p>}
      {!!(window as any).__TAURI_INTERNALS__ && <button type="button" disabled={disabled} onClick={browse} className="mx-auto flex items-center gap-2 rounded-lg bg-claude-hover px-3 py-1.5 text-sm text-claude-text"><FolderPlus size={15} /> Browse folders</button>}
      <div className="flex gap-2 pt-2">
        <input aria-label="Folder path" placeholder="Absolute folder path" value={path} disabled={disabled} onChange={e => setPath(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); if (path.trim()) add([path]); } }} className="min-w-0 flex-1 rounded-lg border border-claude-border bg-transparent px-3 py-2 text-sm text-claude-text outline-none focus:border-blue-500" />
        <button type="button" disabled={disabled || !path.trim()} onClick={() => add([path])} className="rounded-lg bg-claude-hover px-3 text-sm text-claude-text disabled:opacity-40">Add</button>
      </div>
    </div>
    {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
  </div>;
}
