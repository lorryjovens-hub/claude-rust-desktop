import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';
import { useProjectStore } from '../../stores/useProjectStore';

// Apply the selection after resetting the chat, which clears pendingProjectId.
export function useCreatedProject(resetChat: () => void) {
  const location = useLocation();
  const projectId = location.state?.createdProjectId;

  useEffect(() => {
    if (location.pathname !== '/' || typeof projectId !== 'string' || !projectId) return;
    // resetChat also replaces the route state so this selection is consumed once.
    resetChat();
    useProjectStore.getState().setPendingProjectId(projectId);
  }, [location.pathname, projectId, resetChat]);
}
