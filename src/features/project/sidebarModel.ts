import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
  conversationWorkspaceMode,
  WorkspaceMode,
} from "../../stores/useWorkspaceModeStore";

export interface SidebarChat {
  id: string;
  title?: string | null;
  first_message?: string;
  project_id?: string | null;
  workspace_path?: string | null;
  workspace_mode?: string | null;
  pinned?: boolean;
  archived?: boolean;
  updated_at?: string;
  created_at?: string;
}

export const chatTitle = (chat: SidebarChat, fallback = "New Chat") =>
  chat.title || chat.first_message?.slice(0, 40) || fallback;
export function visibleChats(chats: SidebarChat[], mode?: WorkspaceMode) {
  return chats
    .filter(
      (chat) => !chat.archived && (!mode || (conversationWorkspaceMode(chat) === 'code') === (mode === 'code')),
    )
    .sort((a, b) =>
      (b.updated_at || b.created_at || "").localeCompare(
        a.updated_at || a.created_at || "",
      ),
    );
}

interface SidebarPreferences {
  pinnedProjects: string[];
  unreadChats: string[];
  collapsedProjects: string[];
  projectsCollapsed: boolean;
  pinnedCollapsed: boolean;
  toggle: (
    key: "pinnedProjects" | "unreadChats" | "collapsedProjects",
    id: string,
  ) => void;
  markRead: (id: string) => void;
  collapse: (key: "projectsCollapsed" | "pinnedCollapsed") => void;
}

export const useSidebarPreferences = create<SidebarPreferences>()(
  persist(
    (set) => ({
      pinnedProjects: [],
      unreadChats: [],
      collapsedProjects: [],
      projectsCollapsed: false,
      pinnedCollapsed: false,
      toggle: (key, id) =>
        set((state) => ({
          [key]: state[key].includes(id)
            ? state[key].filter((value) => value !== id)
            : [...state[key], id],
        })),
      markRead: (id) =>
        set((state) => ({
          unreadChats: state.unreadChats.filter((value) => value !== id),
        })),
      collapse: (key) => set((state) => ({ [key]: !state[key] })),
    }),
    { name: "project-sidebar-preferences" },
  ),
);
