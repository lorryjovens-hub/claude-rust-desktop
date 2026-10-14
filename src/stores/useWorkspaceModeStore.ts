import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type WorkspaceMode = 'chat' | 'work' | 'study' | 'code';

export function normalizeWorkspaceMode(mode: unknown): WorkspaceMode {
  if (mode === 'cowork') return 'work';
  return mode === 'work' || mode === 'study' || mode === 'code' ? mode : 'chat';
}

export function conversationWorkspaceMode(conversation: { workspace_mode?: string | null }): WorkspaceMode {
  return normalizeWorkspaceMode(conversation.workspace_mode);
}

interface WorkspaceModeState {
  mode: WorkspaceMode;
  lastHomeMode: Exclude<WorkspaceMode, 'code'>;
  recentsCollapsed: Record<WorkspaceMode, boolean>;
  lastCodeProjectId: string | null;
  setMode: (mode: WorkspaceMode) => void;
  setLastCodeProjectId: (id: string | null) => void;
  toggleRecents: (mode: WorkspaceMode) => void;
}

export const useWorkspaceModeStore = create<WorkspaceModeState>()(persist((set) => ({
  mode: 'chat',
  lastHomeMode: 'chat',
  recentsCollapsed: { chat: false, work: false, study: false, code: false },
  lastCodeProjectId: null,
  setMode: (mode) => set(mode === 'code' ? { mode } : { mode, lastHomeMode: mode }),
  setLastCodeProjectId: (lastCodeProjectId) => set({ lastCodeProjectId }),
  toggleRecents: (mode) => set(state => ({
    recentsCollapsed: { ...state.recentsCollapsed, [mode]: !state.recentsCollapsed[mode] },
  })),
}), {
  name: 'workspace-mode',
  version: 1,
  migrate: (persisted) => {
    const previous = persisted as Partial<WorkspaceModeState> & { recentsCollapsed?: Record<string, boolean> };
    return {
      ...previous,
      mode: normalizeWorkspaceMode(previous.mode),
      lastHomeMode: normalizeWorkspaceMode(previous.mode) === 'code' ? 'chat' : normalizeWorkspaceMode(previous.mode) as Exclude<WorkspaceMode, 'code'>,
      recentsCollapsed: {
        chat: previous.recentsCollapsed?.chat ?? false,
        work: previous.recentsCollapsed?.work ?? previous.recentsCollapsed?.cowork ?? false,
        study: previous.recentsCollapsed?.study ?? false,
        code: previous.recentsCollapsed?.code ?? false,
      },
    };
  },
}));
