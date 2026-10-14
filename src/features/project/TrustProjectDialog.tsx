import { useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { X } from 'lucide-react';
import { Project, trustProjectFolders } from '../../api';

export default function TrustProjectDialog({ project, onTrusted, onClose }: {
  project: Project; onTrusted: (project: Project) => void | Promise<void>; onClose: () => void;
}) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const folders = (project.folders || []).filter(f => !f.trusted);
  const trust = async () => {
    setBusy(true); setError('');
    try { await onTrusted(await trustProjectFolders(project.id, folders.map(f => f.path))); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  };
  return <div className="fixed inset-0 z-[250] flex items-center justify-center bg-black/40 p-4" onKeyDown={e => { if (e.key === 'Escape' && !busy) { e.stopPropagation(); onClose(); } }}>
    <div role="dialog" aria-modal="true" aria-labelledby="trust-project-title" className="relative w-[420px] max-w-full rounded-2xl bg-claude-input border border-claude-border p-6 shadow-xl text-center">
      <button type="button" disabled={busy} aria-label={t('projectDialogs.closeTrust')} onClick={onClose} className="absolute right-4 top-4 text-claude-textSecondary"><X size={16} /></button>
      <h2 id="trust-project-title" className="text-lg font-semibold text-claude-text">{t(folders.length === 1 ? 'projectDialogs.trustOneTitle' : 'projectDialogs.trustManyTitle')}</h2>
      <div className="my-4 max-h-40 overflow-y-auto space-y-2 text-sm text-claude-textSecondary">{folders.map(f => <p key={f.path} className="break-all">{f.path}</p>)}</div>
      <p className="mb-5 text-sm leading-relaxed text-claude-textSecondary">{t('projectDialogs.trustDescription')}</p>
      {error && <p role="alert" className="mb-3 text-sm text-red-500">{error}</p>}
      <button type="button" disabled={busy || !folders.length} onClick={trust} className="w-full rounded-full bg-claude-text text-claude-bg py-2 text-sm font-medium disabled:opacity-50">{busy ? t('projectDialogs.saving') : t(folders.length === 1 ? 'projectDialogs.trustOne' : 'projectDialogs.trustMany')}</button>
      <button type="button" autoFocus disabled={busy} onClick={onClose} className="mt-3 w-full rounded-full border border-claude-border py-2 text-sm text-claude-text">{t('common.cancel')}</button>
    </div>
  </div>;
}
