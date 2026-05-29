import { request, API_BASE, getToken, waitForApiReady, isTauriApp, getUserModeForConversation, resolveEnvCreds } from './client';
import {
  getLocalConversations,
  createLocalConversation,
  getLocalConversation,
  updateLocalConversation,
  deleteLocalConversation,
  getLocalMessages,
  saveLocalMessages,
  deleteLocalMessagesFrom,
  deleteLocalMessagesTail,
  LocalConversation,
} from '../services/localStorageService';

function dispatchEvent(name: string, detail: any) {
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(name, { detail }));
  }
}

// ============ getConversations ============

export async function getConversations() {
  console.log('[API] getConversations called, isTauriApp:', isTauriApp);
  if (isTauriApp) {
    try {
      await waitForApiReady();
      const res = await fetch(`${API_BASE}/conversations`, {
        headers: { ...(getToken() ? { 'Authorization': `Bearer ${getToken()}` } : {}) },
      });
      if (!res.ok) {
        console.error('[API] getConversations HTTP error:', res.status);
        const localConvs = getLocalConversations();
        if (localConvs.length > 0) {
          console.log('[API] getConversations fallback to localStorage:', localConvs.length, 'conversations');
          return localConvs.map(c => ({
            id: c.id, title: c.title, model: c.model, provider: c.provider,
            workspace_path: c.workspace_path, project_id: c.project_id,
            research_mode: c.research_mode, pinned: c.pinned, archived: c.archived,
            created_at: c.created_at, updated_at: c.updated_at,
            message_count: c.message_count, messages: [],
          }));
        }
        return [];
      }
      const data = await res.json();
      const convs = Array.isArray(data) ? data : (Array.isArray(data?.conversations) ? data.conversations : []);
      console.log('[API] getConversations from SQLite:', convs.length, 'conversations');
      if (convs.length > 0) {
        return convs.map((c: any) => ({
          id: c.id, title: c.title, model: c.model, provider: c.provider,
          workspace_path: c.workspace_path, project_id: c.project_id,
          research_mode: c.research_mode, pinned: c.pinned, archived: c.archived,
          created_at: c.created_at, updated_at: c.updated_at,
          message_count: c.message_count, messages: [],
        }));
      }
    } catch (e) {
      console.error('[API] getConversations from SQLite failed:', e);
    }
    const localConvs = getLocalConversations();
    if (localConvs.length > 0) {
      console.log('[API] getConversations fallback to localStorage:', localConvs.length, 'conversations');
      return localConvs.map(c => ({
        id: c.id, title: c.title, model: c.model, provider: c.provider,
        workspace_path: c.workspace_path, project_id: c.project_id,
        research_mode: c.research_mode, pinned: c.pinned, archived: c.archived,
        created_at: c.created_at, updated_at: c.updated_at,
        message_count: c.message_count, messages: [],
      }));
    }
    return [];
  }
  // Web mode: load from localStorage
  const localConvs = getLocalConversations();
  console.log('[API] getConversations from localStorage:', localConvs.length, 'conversations');
  return localConvs.map(c => ({
    id: c.id, title: c.title, model: c.model, provider: c.provider,
    workspace_path: c.workspace_path, project_id: c.project_id,
    research_mode: c.research_mode, pinned: c.pinned, archived: c.archived,
    created_at: c.created_at, updated_at: c.updated_at,
    message_count: c.message_count, messages: [],
  }));
}

// ============ createConversation ============

export async function createConversation(title?: string, model?: string, extras?: { research_mode?: boolean }) {
  console.log('[API] createConversation called: model=', model, 'title=', title);
  const modelName = model || 'claude-sonnet-4-6';
  const researchMode = extras?.research_mode || false;

  if (isTauriApp) {
    try {
      await waitForApiReady();
      const body: any = { model: modelName };
      if (title !== undefined) body.title = title;
      if (researchMode) body.research_mode = researchMode;
      const token = getToken();
      const res = await fetch(`${API_BASE}/conversations`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...(token ? { 'Authorization': `Bearer ${token}` } : {}) },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        console.error('[API] createConversation HTTP error:', res.status);
        throw new Error('Failed to create conversation');
      }
      const data = await res.json();
      console.log('[API] createConversation via SQLite:', data.id);
      if (data && data.id) {
        dispatchEvent('conversationCreated', { id: data.id });
        // Also save to localStorage for fallback
        createLocalConversation(title, modelName, { research_mode: researchMode });
        return data;
      }
      console.warn('[API] Bridge createConversation returned no id:', data);
    } catch (e) {
      console.warn('[API] createConversation via SQLite failed:', e);
    }
  }

  // fallback to localStorage
  const conv = createLocalConversation(title || undefined, modelName, { research_mode: researchMode });
  console.log('[API] createConversation via localStorage:', conv.id);
  dispatchEvent('conversationCreated', { id: conv.id });
  return conv;
}

// ============ getConversation ============

export async function getConversation(id: string) {
  console.log('[API] getConversation called for id:', id);
  if (isTauriApp) {
    try {
      await waitForApiReady();
      const res = await fetch(`${API_BASE}/conversations/${id}`, {
        headers: { ...(getToken() ? { 'Authorization': `Bearer ${getToken()}` } : {}) },
      });
      if (!res.ok) {
        console.error('[API] getConversation HTTP error:', res.status);
        const localConv = getLocalConversation(id);
        if (localConv) {
          const localMessages = getLocalMessages(id);
          return {
            id: localConv.id, title: localConv.title, model: localConv.model,
            workspace_path: localConv.workspace_path, created_at: localConv.created_at,
            updated_at: localConv.updated_at, message_count: localConv.message_count,
            messages: localMessages.map(m => ({
              id: m.id, role: m.role || 'assistant', content: m.content,
              thinking: m.thinking || null, created_at: m.created_at,
              is_compact_boundary: m.is_compact_boundary, sort_order: m.sort_order,
              toolCalls: m.toolCalls || [],
            })),
          };
        }
        throw new Error('Conversation not found');
      }
      const data = await res.json();
      console.log('[API] getConversation from SQLite, messages:', data.messages?.length || 0);
      return {
        id: data.id, title: data.title, model: data.model,
        workspace_path: data.workspace_path, created_at: data.created_at,
        updated_at: data.updated_at, message_count: data.message_count,
        messages: (data.messages || []).map((m: any) => {
          let toolCalls: any[] = [];
          if (m.tool_calls) {
            if (typeof m.tool_calls === 'string') {
              try { toolCalls = JSON.parse(m.tool_calls); } catch { toolCalls = []; }
            } else if (Array.isArray(m.tool_calls)) {
              toolCalls = m.tool_calls;
            }
          }
          return {
            id: m.id, role: m.role || 'assistant', content: m.content,
            thinking: m.thinking || null, created_at: m.created_at,
            is_compact_boundary: m.is_compact_boundary, sort_order: m.sort_order,
            toolCalls,
          };
        }),
      };
    } catch (e) {
      console.error('[API] getConversation from SQLite failed:', e);
      const localConv = getLocalConversation(id);
      if (localConv) {
        const localMessages = getLocalMessages(id);
        console.log('[API] getConversation fallback to localStorage, messages:', localMessages.length);
        return {
          id: localConv.id, title: localConv.title, model: localConv.model,
          workspace_path: localConv.workspace_path, created_at: localConv.created_at,
          updated_at: localConv.updated_at, message_count: localConv.message_count,
          messages: localMessages.map(m => ({
            id: m.id, role: m.role || 'assistant', content: m.content,
            thinking: m.thinking || null, created_at: m.created_at,
            is_compact_boundary: m.is_compact_boundary, sort_order: m.sort_order,
            toolCalls: m.toolCalls || [],
          })),
        };
      }
      throw e;
    }
  }
  // Web mode: localStorage
  const conv = getLocalConversation(id);
  if (!conv) throw new Error('Conversation not found');
  const messages = getLocalMessages(id);
  console.log('[API] getConversation from localStorage, messages:', messages.length);
  return {
    id: conv.id, title: conv.title, model: conv.model,
    workspace_path: conv.workspace_path, created_at: conv.created_at,
    updated_at: conv.updated_at, message_count: conv.message_count,
    messages: messages.map(m => ({
      id: m.id, role: m.role || 'assistant', content: m.content,
      thinking: m.thinking || null, created_at: m.created_at,
      is_compact_boundary: m.is_compact_boundary, sort_order: m.sort_order,
      toolCalls: m.toolCalls || [],
    })),
  };
}

// ============ deleteConversation ============

export async function deleteConversation(id: string) {
  if (isTauriApp) {
    dispatchEvent('conversationDeleting', { id });
    try {
      await waitForApiReady();
      const token = getToken();
      await fetch(`${API_BASE}/conversations/${id}`, {
        method: 'DELETE',
        headers: { ...(token ? { 'Authorization': `Bearer ${token}` } : {}) },
      });
    } catch (e) {
      console.error('[API] deleteConversation from SQLite failed:', e);
    }
    dispatchEvent('conversationDeleted', { id });
    return { success: true };
  }
  // Web mode: localStorage
  dispatchEvent('conversationDeleting', { id });
  deleteLocalConversation(id);
  dispatchEvent('conversationDeleted', { id });
  return { success: true };
}

// ============ updateConversation ============

export async function updateConversation(id: string, data: any) {
  if (isTauriApp) {
    try {
      await waitForApiReady();
      const token = getToken();
      await fetch(`${API_BASE}/conversations/${id}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...(token ? { 'Authorization': `Bearer ${token}` } : {}) },
        body: JSON.stringify(data),
      });
    } catch (e) {
      console.error('[API] updateConversation failed:', e);
    }
    return { ...data, id };
  }
  // Web mode: localStorage
  updateLocalConversation(id, data);
  return { ...data, id };
}

// ============ Messages persistence (web mode) ============

export function persistMessages(conversationId: string, messages: any[]) {
  const mapped = messages.map((m, i) => ({
    id: m.id || crypto.randomUUID(),
    conversation_id: conversationId,
    role: m.role,
    content: m.content || '',
    thinking: m.thinking || null,
    created_at: m.created_at || new Date().toISOString(),
    is_compact_boundary: m.is_compact_boundary,
    sort_order: m.sort_order !== undefined ? m.sort_order : i,
    toolCalls: m.toolCalls || [],
  }));
  saveLocalMessages(conversationId, mapped);
}

// ============ Export ============

export async function exportConversation(id: string): Promise<void> {
  const token = getToken();

  if (typeof window !== 'undefined' && (window as any).electronAPI) {
    try {
      const conv = await getConversation(id);
      const lines = [`# ${conv.title || 'Conversation Snapshot'}\n`];
      if (conv.messages && conv.messages.length > 0) {
        conv.messages.forEach((m: any) => {
          lines.push(`## ${m.role === 'user' ? '用户 (User)' : '助手 (Assistant)'} - ${new Date(m.created_at).toLocaleString()}`);
          lines.push(`${m.content}\n`);
          if (m.toolCalls && m.toolCalls.length > 0) {
            lines.push(`> [Tool Executions] ${m.toolCalls.map((tc: any) => tc.name).join(', ')}\n`);
          }
        });
      }
      const contextMarkdown = lines.join('\n');
      const defaultFilename = `conversation-${id.slice(0, 8)}.zip`;
      const result = await (window as any).electronAPI.exportWorkspace(id, contextMarkdown, defaultFilename);
      if (result && !result.success && result.reason !== 'canceled') {
        throw new Error("Local Export Failed");
      }
      return;
    } catch (err: any) {
      console.warn("Electron native export failed:", err);
      throw new Error(err.message || "工作空间生成导致导出失败");
    }
  }

  if (!isTauriApp) {
    const conv = await getConversation(id);
    const lines = [`# ${conv.title || 'Conversation Snapshot'}\n`];
    if (conv.messages && conv.messages.length > 0) {
      conv.messages.forEach((m: any) => {
        lines.push(`## ${m.role === 'user' ? '用户 (User)' : '助手 (Assistant)'} - ${new Date(m.created_at).toLocaleString()}`);
        lines.push(`${m.content}\n`);
      });
    }
    const blob = new Blob([lines.join('\n')], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `conversation-${id.slice(0, 8)}.md`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return;
  }

  await waitForApiReady();
  const res = await fetch(`${API_BASE}/conversations/${id}/export`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (res.status === 401) {
    localStorage.removeItem('auth_token');
    localStorage.removeItem('user');
    window.location.hash = '#/login'; window.location.reload();
    throw new Error('认证失效');
  }
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error((err as any).error || '导出失败');
  }
  const blob = await res.blob();
  const disposition = res.headers.get('content-disposition') || '';
  const utf8Match = disposition.match(/filename\*=UTF-8''([^;]+)/i);
  const plainMatch = disposition.match(/filename="?([^"]+)"?/i);
  const filename = utf8Match
    ? decodeURIComponent(utf8Match[1])
    : (plainMatch ? plainMatch[1] : `conversation-${id.slice(0, 8)}.zip`);
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// ============ Generation / stream helpers (require bridge) ============

export async function getGenerationStatus(conversationId: string) {
  if (!isTauriApp) return { generating: false };
  try {
    const res = await request(`/conversations/${conversationId}/generation-status`);
    return res.json();
  } catch { return { generating: false }; }
}

export async function stopGeneration(conversationId: string) {
  if (!isTauriApp) return { ok: true };
  try {
    const res = await request(`/conversations/${conversationId}/stop-generation`, { method: 'POST' });
    return res.json();
  } catch { return { ok: true }; }
}

export async function getContextSize(conversationId: string): Promise<{ tokens: number; limit: number }> {
  if (!isTauriApp) return { tokens: 0, limit: 200000 };
  const res = await request(`/conversations/${conversationId}/context-size`);
  return res.json();
}

export async function compactConversation(
  id: string,
  instruction?: string
): Promise<{ summary: string; tokensSaved: number; messagesCompacted: number }> {
  const res = await request(`/conversations/${id}/compact`, {
    method: 'POST',
    body: JSON.stringify({
      instruction,
      ...resolveEnvCreds(getUserModeForConversation(id)),
    }),
  });
  return res.json();
}

export async function branchConversation(
  conversationId: string,
  fromMessageId?: string
): Promise<{ success: boolean; new_conversation_id: string }> {
  const res = await request(`/conversations/${conversationId}/branch`, {
    method: 'POST',
    body: JSON.stringify({ from_message_id: fromMessageId }),
  });
  return res.json();
}

export async function answerUserQuestion(
  conversationId: string,
  requestId: string,
  toolUseId: string,
  answers: Record<string, string>
): Promise<{ ok: boolean }> {
  const res = await request(`/conversations/${conversationId}/answer`, {
    method: 'POST',
    body: JSON.stringify({ request_id: requestId, tool_use_id: toolUseId, answers }),
  });
  return res.json();
}

export async function respondToolPermission(
  conversationId: string,
  requestId: string,
  toolUseId: string,
  behavior: 'allow' | 'deny'
): Promise<{ ok: boolean }> {
  const res = await request(`/conversations/${conversationId}/permission`, {
    method: 'POST',
    body: JSON.stringify({ request_id: requestId, tool_use_id: toolUseId, behavior }),
  });
  return res.json();
}

export async function deleteMessagesFrom(
  conversationId: string,
  messageId: string,
  preserveAttachmentIds?: string[]
) {
  if (isTauriApp) {
    const res = await request(`/conversations/${conversationId}/messages/${messageId}`, {
      method: 'DELETE',
      body: preserveAttachmentIds && preserveAttachmentIds.length > 0
        ? JSON.stringify({ preserve_attachment_ids: preserveAttachmentIds })
        : undefined,
    });
    return res.json();
  }
  deleteLocalMessagesFrom(conversationId, messageId);
  return { ok: true };
}

export async function deleteMessagesTail(
  conversationId: string,
  count: number,
  preserveAttachmentIds?: string[]
) {
  if (isTauriApp) {
    const res = await request(`/conversations/${conversationId}/messages-tail/${count}`, {
      method: 'DELETE',
      body: preserveAttachmentIds && preserveAttachmentIds.length > 0
        ? JSON.stringify({ preserve_attachment_ids: preserveAttachmentIds })
        : undefined,
    });
    return res.json();
  }
  deleteLocalMessagesTail(conversationId, count);
  return { ok: true };
}

export async function getStreamStatus(conversationId: string): Promise<{ active: boolean; eventCount: number }> {
  if (!isTauriApp) return { active: false, eventCount: 0 };
  const res = await request(`/conversations/${conversationId}/stream-status`);
  return res.json();
}

export async function reconnectStream(
  conversationId: string,
  onDelta: (delta: string, full: string) => void,
  onDone: (full: string) => void,
  onError: (err: string) => void,
  onThinking?: (thinking: string, full: string) => void,
  onSystem?: (event: string, message: string, data: any) => void,
  onToolUse?: (event: { type: 'start' | 'input' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; content?: string; is_error?: boolean; textBefore?: string }) => void,
  signal?: AbortSignal
): Promise<void> {
  if (!isTauriApp) {
    onError('Reconnect only available in Tauri mode');
    return;
  }
  let fullText = '';
  let thinkingText = '';

  fetch(`${API_BASE}/conversations/${conversationId}/reconnect`, { signal })
    .then(async (res) => {
      if (!res.ok || !res.body) { onError('Reconnect failed'); return; }
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.startsWith('data:')) continue;
          const data = line.startsWith('data: ') ? line.slice(6) : line.slice(5);
          if (data.trim() === '[DONE]') { onDone(fullText); return; }

          try {
            const parsed = JSON.parse(data);

            if (parsed.type === 'content_block_delta' && parsed.delta) {
              if (parsed.delta.type === 'text_delta' && parsed.delta.text) {
                fullText += parsed.delta.text;
                onDelta(parsed.delta.text, fullText);
              }
              if (parsed.delta.type === 'thinking_delta' && parsed.delta.thinking && onThinking) {
                thinkingText += parsed.delta.thinking;
                onThinking(parsed.delta.thinking, thinkingText);
              }
            }
            if (parsed.type === 'tool_use_start' && onToolUse) {
              onToolUse({ type: 'start', tool_use_id: parsed.tool_use_id, tool_name: parsed.tool_name, tool_input: parsed.tool_input, textBefore: parsed.textBefore || '' });
            }
            if (parsed.type === 'tool_use_input' && onToolUse) {
              onToolUse({ type: 'input', tool_use_id: parsed.tool_use_id, tool_input: parsed.tool_input });
            }
            if (parsed.type === 'tool_use_done' && onToolUse) {
              onToolUse({ type: 'done', tool_use_id: parsed.tool_use_id, content: parsed.content, is_error: parsed.is_error });
            }
            if (parsed.type === 'ask_user' && onSystem) {
              onSystem('ask_user', '', parsed);
            }
            if (parsed.type === 'tool_permission' && onSystem) {
              onSystem('tool_permission', '', parsed);
            }
            if (parsed.type === 'message_start' && onSystem) {
              onSystem('message_start', '', parsed);
            }
            if (parsed.type === 'message_delta' && onSystem) {
              onSystem('message_delta', '', parsed);
            }
            if (parsed.type === 'task_event' && onSystem) {
              onSystem('task_event', '', parsed);
            }
            if (parsed.type === 'compact_boundary' && onSystem) {
              onSystem('compact_boundary', '', parsed);
            }
            if (parsed.type && parsed.type.startsWith('research_') && onSystem) {
              onSystem(parsed.type, '', parsed);
              if (parsed.type === 'research_report_delta' && parsed.text) {
                fullText += parsed.text;
                onDelta(parsed.text, fullText);
              }
            }
            if (parsed.type === 'message_stop') {
              if (fullText) { onDone(fullText); return; }
            }
            if (parsed.type === 'error') {
              onError(parsed.error || 'Stream error');
              return;
            }
          } catch (_) {}
        }
      }
    })
    .catch((err) => {
      if (err.name !== 'AbortError') onError(err.message || 'Reconnect failed');
    });
}