import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

interface AuthState {
  user: any | null;
  token: string | null;
  hasSubscription: boolean | null;
  authChecked: boolean;
  showLoginRequired: boolean;

  setUser: (user: any | null) => void;
  setToken: (token: string | null) => void;
  setHasSubscription: (has: boolean | null) => void;
  setAuthChecked: (checked: boolean) => void;
  setShowLoginRequired: (show: boolean) => void;
  logout: () => void;
}

const AUTH_STORAGE_KEY = 'auth_state';

const loadPersistedAuth = (): Partial<AuthState> => {
  try {
    const raw = localStorage.getItem(AUTH_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        user: parsed.user ?? null,
        token: parsed.token ?? null,
        hasSubscription: parsed.hasSubscription ?? false,
      };
    }
  } catch {}
  return {};
};

const persistAuth = (state: Pick<AuthState, 'user' | 'token' | 'hasSubscription'>) => {
  try {
    localStorage.setItem(
      AUTH_STORAGE_KEY,
      JSON.stringify({
        user: state.user,
        token: state.token,
        hasSubscription: state.hasSubscription,
      })
    );
  } catch {}
};

const persisted = loadPersistedAuth();

export const useAuthStore = create<AuthState>()(
  subscribeWithSelector((set) => ({
    user: persisted.user ?? null,
    token: persisted.token ?? null,
    hasSubscription: persisted.hasSubscription ?? null,
    authChecked: false,
    showLoginRequired: false,

    setUser: (user) =>
      set((state) => {
        const next = { user };
        persistAuth({ ...state, ...next });
        return next;
      }),

    setToken: (token) =>
      set((state) => {
        const next = { token };
        persistAuth({ ...state, ...next });
        return next;
      }),

    setHasSubscription: (hasSubscription) =>
      set((state) => {
        const next = { hasSubscription };
        persistAuth({ ...state, ...next });
        return next;
      }),

    setAuthChecked: (authChecked) => set({ authChecked }),
    setShowLoginRequired: (showLoginRequired) => set({ showLoginRequired }),

    logout: () => {
      set({
        user: null,
        token: null,
        hasSubscription: false,
        authChecked: true,
        showLoginRequired: false,
      });
    },
  }))
);

useAuthStore.subscribe(
  (state) => ({ user: state.user, token: state.token, hasSubscription: state.hasSubscription }),
  (snapshot) => {
    persistAuth(snapshot);
  }
);
