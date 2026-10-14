import { useCallback, useEffect, useState } from 'react';
import { getConversationChanges, type RepositoryChanges } from '../../api/git';

export function useConversationGit(id: string | null, projectId: string | null, loading: boolean) {
  const [state, setState] = useState<{ id: string; projectId: string; data: RepositoryChanges | null; error: string }>({ id: '', projectId: '', data: null, error: '' });
  const [version, setVersion] = useState(0);
  const refresh = useCallback(() => setVersion(value => value + 1), []);
  useEffect(() => {
    if (!id || !projectId) return;
    let cancelled = false;
    let pending = false;
    const update = async () => {
      if (pending || cancelled) return;
      pending = true;
      try {
        const data = await getConversationChanges(id);
        if (!cancelled) setState({ id, projectId, data, error: '' });
      } catch (error) {
        if (!cancelled) setState({ id, projectId, data: null, error: error instanceof Error ? error.message : String(error) });
      } finally { pending = false; }
    };
    void update();
    const timer = window.setInterval(() => { if (document.visibilityState !== 'hidden') void update(); }, 5000);
    window.addEventListener('focus', update);
    window.addEventListener('projectsChanged', update);
    return () => { cancelled = true; window.clearInterval(timer); window.removeEventListener('focus', update); window.removeEventListener('projectsChanged', update); };
  }, [id, projectId, loading, version]);
  const current = state.id === id && state.projectId === projectId;
  return { data: current ? state.data : null, error: current ? state.error : '', refresh };
}
