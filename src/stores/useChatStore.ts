import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface Message {
  role: string;
  content: string | any[];
  thinking?: string;
  toolUse?: any;
  toolResult?: any;
}

interface ModelCatalog {
  common: any[];
  all: any[];
  fallback_model: string | null;
}

interface CrossModeWarning {
  convId: string;
  originalModel: string;
  otherMode: 'clawparrot' | 'selfhosted';
  fallbackModel: string;
}

interface CompactStatus {
  state: 'idle' | 'compacting' | 'done' | 'error';
  message?: string;
}

interface ChatState {
  messages: any[];
  loading: boolean;
  inputText: string;
  conversationTitle: string;
  conversationId: string | null;
  modelCatalog: ModelCatalog | null;
  currentModel: string;
  userMode: string;
  researchMode: boolean;
  openedResearchMsgId: string | null;
  compactStatus: CompactStatus;
  compactInstruction: string;
  planMode: boolean;
  crossModeWarning: CrossModeWarning | null;
  providersCache: any[];
  webSearchToast: string | null;
  permissionMode: string;

  setMessages: (messages: any[] | ((prev: any[]) => any[])) => void;
  appendMessage: (msg: any) => void;
  updateLastMessage: (updater: (msg: any) => any) => void;
  appendToLastMessage: (delta: string) => void;
  appendThinkingToLastMessage: (thinking: string) => void;
  setLoading: (loading: boolean) => void;
  setInputText: (text: string | ((prev: string) => string)) => void;
  setConversationTitle: (title: string) => void;
  setConversationId: (id: string | null) => void;
  setModelCatalog: (catalog: ModelCatalog | null) => void;
  setCurrentModel: (model: string | ((prev: string) => string)) => void;
  setUserMode: (mode: string) => void;
  setResearchMode: (mode: boolean) => void;
  setOpenedResearchMsgId: (id: string | null) => void;
  setCompactStatus: (status: CompactStatus) => void;
  setCompactInstruction: (instruction: string) => void;
  setPlanMode: (mode: boolean) => void;
  setCrossModeWarning: (warning: CrossModeWarning | null) => void;
  setProvidersCache: (providers: any[]) => void;
  setWebSearchToast: (toast: string | null) => void;
  resetChat: () => void;
}

const initialState = {
  messages: [] as any[],
  loading: false,
  inputText: '',
  conversationTitle: '',
  conversationId: null as string | null,
  modelCatalog: null as ModelCatalog | null,
  currentModel: '',
  userMode: '',
  researchMode: false,
  openedResearchMsgId: null as string | null,
  compactStatus: { state: 'idle' as const },
  compactInstruction: '',
  planMode: false,
  crossModeWarning: null as CrossModeWarning | null,
  providersCache: [] as any[],
  webSearchToast: null as string | null,
  permissionMode: (() => {
    try {
      return localStorage.getItem('permission_mode') || 'accept_edits';
    } catch { return 'accept_edits'; }
  })() as string,
};

export const useChatStore = create<ChatState>()(
  subscribeWithSelector((set) => ({
    ...initialState,

    setMessages: (messages) =>
      set((state) => ({
        messages: typeof messages === 'function' ? messages(state.messages) : messages,
      })),

    appendMessage: (msg) =>
      set((state) => ({ messages: [...state.messages, msg] })),

    updateLastMessage: (updater) =>
      set((state) => {
        if (state.messages.length === 0) return state;
        const lastIdx = state.messages.length - 1;
        const updated = updater(state.messages[lastIdx]);
        if (updated === state.messages[lastIdx]) return state;
        const messages = [...state.messages];
        messages[lastIdx] = updated;
        return { messages };
      }),

    appendToLastMessage: (delta) =>
      set((state) => {
        if (state.messages.length === 0) return state;
        const lastIdx = state.messages.length - 1;
        const lastMsg = state.messages[lastIdx];
        if (typeof lastMsg.content !== 'string') return state;
        const newContent = lastMsg.content + delta;
        if (newContent === lastMsg.content) return state;
        const messages = [...state.messages];
        messages[lastIdx] = { ...lastMsg, content: newContent };
        return { messages };
      }),

    appendThinkingToLastMessage: (thinking) =>
      set((state) => {
        if (state.messages.length === 0) return state;
        const lastIdx = state.messages.length - 1;
        const lastMsg = state.messages[lastIdx];
        const newThinking = (lastMsg.thinking ?? '') + thinking;
        if (newThinking === lastMsg.thinking) return state;
        const messages = [...state.messages];
        messages[lastIdx] = { ...lastMsg, thinking: newThinking };
        return { messages };
      }),

    setLoading: (loading) => set({ loading }),

    setInputText: (inputText) =>
      set((state) => ({
        inputText: typeof inputText === 'function' ? inputText(state.inputText) : inputText,
      })),

    setConversationTitle: (conversationTitle) => set({ conversationTitle }),
    setConversationId: (conversationId) => set({ conversationId }),
    setModelCatalog: (modelCatalog) => set({ modelCatalog }),

    setCurrentModel: (currentModel) =>
      set((state) => ({
        currentModel: typeof currentModel === 'function' ? currentModel(state.currentModel) : currentModel,
      })),

    setUserMode: (userMode) => set({ userMode }),
    setResearchMode: (researchMode) => set({ researchMode }),
    setOpenedResearchMsgId: (openedResearchMsgId) => set({ openedResearchMsgId }),
    setCompactStatus: (compactStatus) => set({ compactStatus }),
    setCompactInstruction: (compactInstruction) => set({ compactInstruction }),
    setPlanMode: (planMode) => set({ planMode }),
    setCrossModeWarning: (crossModeWarning) => set({ crossModeWarning }),
    setProvidersCache: (providersCache) => set({ providersCache }),
    setWebSearchToast: (webSearchToast) => set({ webSearchToast }),
    setPermissionMode: (permissionMode: string) => {
      set({ permissionMode });
      // Persist to localStorage
      try {
        localStorage.setItem('permission_mode', permissionMode);
      } catch {}
    },
    resetChat: () => set(initialState),
  }))
);

let pendingDelta = '';
let deltaRafId: number | null = null;

const flushDelta = () => {
  if (pendingDelta) {
    useChatStore.getState().appendToLastMessage(pendingDelta);
    pendingDelta = '';
  }
  deltaRafId = null;
};

export function appendDeltaThrottled(delta: string) {
  pendingDelta += delta;
  if (!deltaRafId) {
    deltaRafId = requestAnimationFrame(flushDelta);
  }
}

let pendingThinking = '';
let thinkingRafId: number | null = null;

const flushThinking = () => {
  if (pendingThinking) {
    useChatStore.getState().appendThinkingToLastMessage(pendingThinking);
    pendingThinking = '';
  }
  thinkingRafId = null;
};

export function appendThinkingThrottled(thinking: string) {
  pendingThinking += thinking;
  if (!thinkingRafId) {
    thinkingRafId = requestAnimationFrame(flushThinking);
  }
}
