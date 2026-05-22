import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

const isTauri = typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

export interface EngineTextEvent {
  conversation_id: string;
  event: { type: 'text'; text: string };
}

export interface EngineThinkingEvent {
  conversation_id: string;
  event: { type: 'thinking'; text: string };
}

export interface EngineToolUseStartEvent {
  conversation_id: string;
  event: { type: 'tool_use_start'; tool_use_id: string; tool_name: string; tool_input: any };
}

export interface EngineToolUseDoneEvent {
  conversation_id: string;
  event: { type: 'tool_use_done'; tool_use_id: string; tool_name: string; tool_input: any; output: string; is_error: boolean };
}

export interface EngineMessageStartEvent {
  conversation_id: string;
  event: { type: 'message_start'; model: string };
}

export interface EngineMessageDeltaEvent {
  conversation_id: string;
  event: { type: 'message_delta'; stop_reason: string | null };
}

export interface EngineMessageStopEvent {
  conversation_id: string;
  event: { type: 'message_stop' };
}

export interface EngineErrorEvent {
  conversation_id: string;
  event: { type: 'error'; error: string };
}

export interface EngineUsageEvent {
  conversation_id: string;
  event: { type: 'usage'; usage: any };
}

export type EngineEvent =
  | EngineTextEvent
  | EngineThinkingEvent
  | EngineToolUseStartEvent
  | EngineToolUseDoneEvent
  | EngineMessageStartEvent
  | EngineMessageDeltaEvent
  | EngineMessageStopEvent
  | EngineErrorEvent
  | EngineUsageEvent;

export interface NativeEngineState {
  initialized: boolean;
  provider_count: number;
  conversation_count: number;
}

export interface NativeChatRequest {
  conversation_id: string;
  messages: any[];
  model: string;
  system_prompt?: string;
  max_tokens?: number;
}

export interface NativeChatResponse {
  conversation_id: string;
  status: string;
}

export interface CreateConversationRequest {
  model: string;
  title?: string;
  research_mode?: boolean;
}

export interface ConversationInfo {
  id: string;
  title: string | null;
  model: string;
  workspace_path: string;
  created_at: string;
  updated_at: string;
}

export interface ToolCallInfo {
  id: string;
  name: string;
  input: any;
  output: string | null;
  is_error: boolean | null;
}

export interface MessageInfo {
  id: string;
  conversation_id: string;
  role: string;
  content: string;
  created_at: string;
  tool_calls: ToolCallInfo[] | null;
}

export interface ProviderInfo {
  id: string;
  name: string;
  base_url: string;
  api_format: string;
  enabled: boolean;
  models: Array<{ id: string; name: string; enabled: boolean }>;
}

export interface UpdateProviderRequest {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
  api_format: string;
  models: Array<{ id: string; name: string; enabled: boolean }>;
  enabled: boolean;
}

export const nativeEngineAPI = {
  async init(): Promise<NativeEngineState> {
    return invoke('native_engine_init');
  },

  async chat(request: NativeChatRequest): Promise<NativeChatResponse> {
    return invoke('native_chat', { request });
  },

  async createConversation(request: CreateConversationRequest): Promise<ConversationInfo> {
    return invoke('native_create_conversation', { request });
  },

  async listConversations(): Promise<ConversationInfo[]> {
    return invoke('native_list_conversations');
  },

  async deleteConversation(conversationId: string): Promise<void> {
    return invoke('native_delete_conversation', { conversationId });
  },

  async getMessages(conversationId: string): Promise<MessageInfo[]> {
    return invoke('native_get_messages', { conversationId });
  },

  async listProviders(): Promise<ProviderInfo[]> {
    return invoke('native_list_providers');
  },

  async updateProvider(request: UpdateProviderRequest): Promise<void> {
    return invoke('native_update_provider', { request });
  },

  async deleteProvider(id: string): Promise<void> {
    return invoke('native_delete_provider', { id });
  },

  onText(handler: (event: EngineTextEvent) => void): Promise<UnlistenFn> {
    return listen('engine:text', (e) => handler(e.payload as EngineTextEvent));
  },

  onThinking(handler: (event: EngineThinkingEvent) => void): Promise<UnlistenFn> {
    return listen('engine:thinking', (e) => handler(e.payload as EngineThinkingEvent));
  },

  onToolUseStart(handler: (event: EngineToolUseStartEvent) => void): Promise<UnlistenFn> {
    return listen('engine:tool_use_start', (e) => handler(e.payload as EngineToolUseStartEvent));
  },

  onToolUseDone(handler: (event: EngineToolUseDoneEvent) => void): Promise<UnlistenFn> {
    return listen('engine:tool_use_done', (e) => handler(e.payload as EngineToolUseDoneEvent));
  },

  onMessageStart(handler: (event: EngineMessageStartEvent) => void): Promise<UnlistenFn> {
    return listen('engine:message_start', (e) => handler(e.payload as EngineMessageStartEvent));
  },

  onMessageDelta(handler: (event: EngineMessageDeltaEvent) => void): Promise<UnlistenFn> {
    return listen('engine:message_delta', (e) => handler(e.payload as EngineMessageDeltaEvent));
  },

  onMessageStop(handler: (event: EngineMessageStopEvent) => void): Promise<UnlistenFn> {
    return listen('engine:message_stop', (e) => handler(e.payload as EngineMessageStopEvent));
  },

  onError(handler: (event: EngineErrorEvent) => void): Promise<UnlistenFn> {
    return listen('engine:error', (e) => handler(e.payload as EngineErrorEvent));
  },

  onUsage(handler: (event: EngineUsageEvent) => void): Promise<UnlistenFn> {
    return listen('engine:usage', (e) => handler(e.payload as EngineUsageEvent));
  },
};

export const tauriAPI = {
  isTauri,

  async getPlatform(): Promise<{ os: string; arch: string; is_electron: boolean }> {
    if (!isTauri) return { os: 'web', arch: 'unknown', is_electron: false };
    return invoke('get_platform');
  },

  async getAppPath(): Promise<string> {
    if (!isTauri) return '';
    return invoke('get_app_path');
  },

  async selectDirectory(): Promise<string | null> {
    if (!isTauri) return null;
    return invoke('select_directory');
  },

  async showItemInFolder(path: string): Promise<void> {
    if (!isTauri) return;
    return invoke('show_item_in_folder', { path });
  },

  async openFolder(path: string): Promise<void> {
    if (!isTauri) return;
    return invoke('open_folder', { path });
  },

  async openExternal(url: string): Promise<void> {
    if (!isTauri) {
      window.open(url, '_blank');
      return;
    }
    return invoke('open_external_url', { url });
  },

  async resizeWindow(width: number, height: number): Promise<void> {
    if (!isTauri) return;
    return invoke('resize_window', { width, height });
  },

  async showMainWindow(): Promise<void> {
    if (!isTauri) return;
    return invoke('show_main_window');
  },

  async exportWorkspace(
    workspaceId: string,
    contextMarkdown: string,
    defaultFilename: string
  ): Promise<string> {
    if (!isTauri) return '';
    return invoke('export_workspace', { workspaceId, contextMarkdown, defaultFilename });
  },

  async getSystemStatus(): Promise<{
    platform: string;
    git_bash: { required: boolean; found: boolean; path: string | null };
  }> {
    if (!isTauri) {
      return {
        platform: 'web',
        git_bash: { required: false, found: false, path: null },
      };
    }
    return invoke('get_system_status');
  },

  async executeTool(
    name: string,
    input: any,
    cwd?: string
  ): Promise<any> {
    if (!isTauri) return null;
    return invoke('execute_tool', { name, input, cwd });
  },

  async checkUpdate(): Promise<{ has_update: boolean }> {
    if (!isTauri) return { has_update: false };
    return invoke('check_update');
  },

  async installUpdate(): Promise<void> {
    if (!isTauri) return;
    return invoke('install_update');
  },
};
