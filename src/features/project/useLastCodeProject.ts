import { useEffect } from 'react';
import { getProjects } from '../../api';
import { useProjectStore } from '../../stores/useProjectStore';
import { useWorkspaceModeStore } from '../../stores/useWorkspaceModeStore';

export function useLastCodeProject(activeConversationId: string | null) {
  const mode = useWorkspaceModeStore(s => s.mode);
  const lastProjectId = useWorkspaceModeStore(s => s.lastCodeProjectId);
  const pendingProjectId = useProjectStore(s => s.pendingProjectId);

  useEffect(() => {
    if (mode !== 'code' || activeConversationId || pendingProjectId || !lastProjectId) return;
    let cancelled = false;
    getProjects().then(projects => {
      if (cancelled || useProjectStore.getState().pendingProjectId) return;
      const project = projects.find(p => p.id === lastProjectId && !p.is_archived);
      if (project) {
        // Selecting a default does not grant folder trust; sending still checks it.
        useProjectStore.getState().setPendingProjectId(project.id);
      } else {
        useWorkspaceModeStore.getState().setLastCodeProjectId(null);
      }
    }).catch(() => { /* Keep the preference if the backend is temporarily unavailable. */ });
    return () => { cancelled = true; };
  }, [mode, activeConversationId, pendingProjectId, lastProjectId]);
}
