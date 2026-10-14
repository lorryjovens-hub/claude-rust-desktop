import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type WorkspaceMode = 'chat' | 'cowork' | 'code';

export function conversationWorkspaceMode(conversation: { workspace_mode?: string | null }): WorkspaceMode {
  return conversation.workspace_mode === 'cowork' || conversation.workspace_mode === 'code'
    ? conversation.workspace_mode : 'chat';
}

interface WorkspaceModeState {
  mode: WorkspaceMode;
  recentsCollapsed: Record<WorkspaceMode, boolean>;
  setMode: (mode: WorkspaceMode) => void;
  toggleRecents: (mode: WorkspaceMode) => void;
}

export const useWorkspaceModeStore = create<WorkspaceModeState>()(persist((set) => ({
  mode: 'chat',
  recentsCollapsed: { chat: false, cowork: false, code: false },
  setMode: (mode) => set({ mode }),
  toggleRecents: (mode) => set(state => ({
    recentsCollapsed: { ...state.recentsCollapsed, [mode]: !state.recentsCollapsed[mode] },
  })),
}), { name: 'workspace-mode' }));
