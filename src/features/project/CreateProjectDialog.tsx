import { useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { X } from 'lucide-react';
import { createProject, Project } from '../../api';
import FolderFields from './FolderFields';
import TrustProjectDialog from './TrustProjectDialog';

export default function CreateProjectDialog({ onCreated, onClose }: {
  onCreated: (project: Project) => void | Promise<void>; onClose: () => void;
}) {
  const { t } = useI18n();
  const [name, setName] = useState('');
  const [folders, setFolders] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [created, setCreated] = useState<Project | null>(null);
  const create = async () => {
    if (!name.trim() || !folders.length || busy) return;
    setBusy(true); setError('');
    try {
      const project = await createProject(name.trim(), '', folders);
      window.dispatchEvent(new Event('projectsChanged'));
      setCreated(project);
    } catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  if (created) return <TrustProjectDialog project={created} onClose={onClose} onTrusted={onCreated} />;
  return <div className="fixed inset-0 z-[220] flex items-center justify-center bg-black/40 p-4" onKeyDown={e => { if (e.key === 'Escape' && !busy) { e.stopPropagation(); onClose(); } }}>
    <form role="dialog" aria-modal="true" aria-labelledby="create-project-title" onSubmit={e => { e.preventDefault(); create(); }} className="w-[520px] max-w-full max-h-[85vh] overflow-y-auto rounded-2xl border border-claude-border bg-claude-bg p-5 shadow-xl">
      <div className="flex items-center justify-between mb-4">
        <h2 id="create-project-title" className="text-xl font-semibold text-claude-text">{t('sidebar.createProject')}</h2>
        <button type="button" disabled={busy} aria-label={t('projectDialogs.closeCreate')} onClick={onClose} className="text-claude-textSecondary"><X size={18} /></button>
      </div>
      <input autoFocus aria-label={t('projectDialogs.name')} placeholder={t('projectDialogs.name')} value={name} disabled={busy} onChange={e => setName(e.target.value)} className="w-full mb-4 rounded-xl border border-claude-border bg-claude-input px-4 py-3 text-sm text-claude-text outline-none focus:border-blue-500" />
      <FolderFields folders={folders} onChange={setFolders} disabled={busy} />
      {error && <p role="alert" className="mt-3 text-sm text-red-500">{error}</p>}
      <div className="flex justify-end gap-3 mt-6">
        <button type="button" disabled={busy} onClick={onClose} className="px-4 py-2 text-sm text-claude-textSecondary">{t('common.cancel')}</button>
        <button type="submit" disabled={busy || !name.trim() || !folders.length} className="rounded-xl bg-claude-text px-4 py-2 text-sm font-medium text-claude-bg disabled:opacity-40">{busy ? t('projectDialogs.creating') : t('sidebar.createProject')}</button>
      </div>
    </form>
  </div>;
}
