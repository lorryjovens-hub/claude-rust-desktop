import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface EnabledSkill {
  id: string;
  name: string;
  description?: string;
}

interface SelectedSkill {
  name: string;
  slug: string;
  description?: string;
}

interface ProjectState {
  projectList: any[];
  currentProjectId: string | null;
  pendingProjectId: string | null;
  enabledSkills: EnabledSkill[];
  selectedSkill: SelectedSkill | null;
  contextInfo: any | null;
  tokenUsage: any | null;

  setProjectList: (list: any[] | ((prev: any[]) => any[])) => void;
  setCurrentProjectId: (id: string | null) => void;
  setPendingProjectId: (id: string | null) => void;
  setEnabledSkills: (skills: EnabledSkill[]) => void;
  setSelectedSkill: (skill: SelectedSkill | null) => void;
  setContextInfo: (info: any | null) => void;
  setTokenUsage: (usage: any | null | ((prev: any | null) => any | null)) => void;
  resetProject: () => void;
}

const initialState = {
  projectList: [] as any[],
  currentProjectId: null as string | null,
  pendingProjectId: null as string | null,
  enabledSkills: [] as EnabledSkill[],
  selectedSkill: null as SelectedSkill | null,
  contextInfo: null as any | null,
  tokenUsage: null as any | null,
};

export const useProjectStore = create<ProjectState>()(
  subscribeWithSelector((set) => ({
    ...initialState,

    setProjectList: (projectList) =>
      set((state) => ({
        projectList: typeof projectList === 'function' ? projectList(state.projectList) : projectList,
      })),
    setCurrentProjectId: (currentProjectId) => set({ currentProjectId }),
    setPendingProjectId: (pendingProjectId) => set({ pendingProjectId }),
    setEnabledSkills: (enabledSkills) => set({ enabledSkills }),
    setSelectedSkill: (selectedSkill) => set({ selectedSkill }),
    setContextInfo: (contextInfo) => set({ contextInfo }),
    setTokenUsage: (tokenUsage) =>
      set((state) => ({
        tokenUsage: typeof tokenUsage === 'function' ? tokenUsage(state.tokenUsage) : tokenUsage,
      })),
    resetProject: () => set(initialState),
  }))
);
