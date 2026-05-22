const DEFAULT_BRIDGE_PORT = 30080;
let detectedBridgePort: number | null = null;

async function detectBridgePort(): Promise<number> {
  if (detectedBridgePort) return detectedBridgePort;
  console.log('[API] Starting Bridge port detection...');
  for (let port = DEFAULT_BRIDGE_PORT; port < DEFAULT_BRIDGE_PORT + 10; port++) {
    console.log(`[API] Trying port ${port}...`);
    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/system-status`, { signal: AbortSignal.timeout(1500) });
      if (res.ok) {
        detectedBridgePort = port;
        console.log(`[API] Bridge found on port ${port}`);
        console.log(`[API] Bridge detection complete. Using port ${port}`);
        return port;
      }
    } catch {
      console.log(`[API] Port ${port} not available`);
    }
  }
  console.log(`[API] Bridge detection complete. Using default port ${DEFAULT_BRIDGE_PORT}`);
  return DEFAULT_BRIDGE_PORT;
}

function getApiBase(): string {
  if (detectedBridgePort && detectedBridgePort !== DEFAULT_BRIDGE_PORT) {
    return `http://127.0.0.1:${detectedBridgePort}/api`;
  }
  return `http://127.0.0.1:${DEFAULT_BRIDGE_PORT}/api`;
}

let API_BASE = `http://127.0.0.1:${DEFAULT_BRIDGE_PORT}/api`;
const GATEWAY_BASE = 'http://127.0.0.1:30090';
const CHENGDU_API = 'http://127.0.0.1:30090/api';
const isTauriApp = typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;

if (isTauriApp) {
  detectBridgePort().then(port => {
    API_BASE = `http://127.0.0.1:${port}/api`;
    detectedBridgePort = port;
  });
}

let nativeEngineInitialized = false;

async function ensureNativeEngine() {
  if (isTauriApp && !nativeEngineInitialized) {
    try {
      const { nativeEngineAPI } = await import('./utils/tauriAPI');
      await nativeEngineAPI.init();
      nativeEngineInitialized = true;
    } catch (e) {
      console.warn('[API] Failed to initialize native engine:', e);
    }
  }
}

// 获取存储的 token
function getToken() {
  return localStorage.getItem('auth_token');
}

// Resolve effective user_mode for a given conversation. If the user has explicitly
// opted to use a cross-mode model in this conversation (via the cross-mode warning
// modal in MainContent), the per-conv override takes precedence over the global
// user_mode. This is what makes "keep using clawparrot opus while in selfhosted
// mode" work — only that one conv switches its endpoint, the rest stay in the
// global mode.
function getUserModeForConversation(conversationId?: string): string {
  if (conversationId) {
    try {
      const raw = localStorage.getItem('cross_mode_overrides');
      if (raw) {
        const map = JSON.parse(raw);
        if (map[conversationId]) return map[conversationId];
      }
    } catch {}
  }
  return localStorage.getItem('user_mode') || 'clawparrot';
}

// Resolve env_token / env_base_url to send to bridge. clawparrot mode must ignore
// CUSTOM_API_KEY/CUSTOM_BASE_URL — those exist only because an old version of the
// app let clawparrot users paste their own relay API key; the UI was removed but
// the localStorage values stick around, and if we fall back to them the user
// silently keeps hitting their old personal relay instead of the clawparrot
// gateway. selfhosted mode still prefers CUSTOM_* since self-deploy users legitimately
// need to bring their own key.
function resolveEnvCreds(mode: string): { env_token?: string; env_base_url?: string } {
  if (mode === 'clawparrot') {
    return {
      env_token: localStorage.getItem('ANTHROPIC_API_KEY') || undefined,
      env_base_url: localStorage.getItem('ANTHROPIC_BASE_URL') || undefined,
    };
  }
  return {
    env_token: localStorage.getItem('CUSTOM_API_KEY') || localStorage.getItem('ANTHROPIC_API_KEY') || undefined,
    env_base_url: localStorage.getItem('CUSTOM_BASE_URL') || localStorage.getItem('ANTHROPIC_BASE_URL') || undefined,
  };
}

// 通用请求方法
async function request(path: string, options: RequestInit = {}) {
  const token = getToken();
  const headers: HeadersInit = {
    'Content-Type': 'application/json',
    ...(options.headers || {}),
  };
  if (token) {
    (headers as any)['Authorization'] = `Bearer ${token}`;
  }
  const res = await fetch(`${API_BASE}${path}`, { ...options, headers });
  if (res.status === 401) {
    localStorage.removeItem('auth_token');
    localStorage.removeItem('user');
    window.location.hash = '#/login'; window.location.reload();
    throw new Error('认证失效');
  }
  if (!res.ok) {
    const errorData = await res.json().catch(() => ({}));
    throw new Error(errorData.error || `Request failed: ${res.status}`);
  }
  return res;
}

// 系统状态（检测 git-bash 等运行时依赖）
export async function getSystemStatus(): Promise<{
  platform: string;
  gitBash: { required: boolean; found: boolean; path: string | null };
}> {
  const res = await fetch(`${API_BASE}/system-status`);
  if (!res.ok) throw new Error('Failed to get system status');
  return res.json();
}

// 认证相关
export async function sendCode(email: string) {
  const res = await request('/auth/send-code', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
  return res.json();
}

export async function register(email: string, password: string, nickname: string, code: string) {
  const res = await request('/auth/register', {
    method: 'POST',
    body: JSON.stringify({ email, password, nickname, code }),
  });
  return res.json();
}

export async function login(email: string, password: string) {
  const res = await request('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
  return res.json();
}

// Gateway login for Electron app — authenticates via local API proxy, returns API key for Claude Code SDK
export async function gatewayLogin(email: string, password: string) {
  const res = await fetch(`${GATEWAY_BASE}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || '登录失败');
  }
  const data = await res.json();
  if (data.token) {
    localStorage.setItem('ANTHROPIC_API_KEY', data.token);
    localStorage.setItem('ANTHROPIC_BASE_URL', GATEWAY_BASE);
    localStorage.setItem('gateway_user', JSON.stringify(data.user || {}));
    if (data.apiKey) {
      localStorage.setItem('CUSTOM_API_KEY', data.apiKey);
    }
    localStorage.setItem('auth_token', data.token);
    if (data.user) {
      localStorage.setItem('user', JSON.stringify(data.user));
    }
  }
  return data;
}

// Check if user is logged in via gateway
export function isGatewayLoggedIn(): boolean {
  return !!(localStorage.getItem('ANTHROPIC_API_KEY') && localStorage.getItem('gateway_user'));
}

// Gateway logout
export function gatewayLogout() {
  localStorage.removeItem('ANTHROPIC_API_KEY');
  localStorage.removeItem('ANTHROPIC_BASE_URL');
  localStorage.removeItem('gateway_user');
  localStorage.removeItem('gateway_quota');
}

// Get gateway usage status
export async function getGatewayUsage() {
  const key = localStorage.getItem('ANTHROPIC_API_KEY');
  if (!key) return null;
  const res = await fetch(`${GATEWAY_BASE}/gateway/usage`, {
    headers: { 'x-api-key': key },
  });
  if (!res.ok) return null;
  return res.json();
}

export async function forgotPassword(email: string) {
  const res = await request('/auth/forgot-password', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
  return res.json();
}

export async function resetPassword(email: string, code: string, password: string) {
  const res = await request('/auth/reset-password', {
    method: 'POST',
    body: JSON.stringify({ email, code, password }),
  });
  return res.json();
}

export function logout() {
  localStorage.removeItem('auth_token');
  localStorage.removeItem('user');
  // Also clear gateway credentials (Electron app)
  localStorage.removeItem('ANTHROPIC_API_KEY');
  localStorage.removeItem('ANTHROPIC_BASE_URL');
  localStorage.removeItem('gateway_user');
  localStorage.removeItem('gateway_quota');
  window.location.hash = '#/login'; window.location.reload();
}

export function getUser() {
  const userStr = localStorage.getItem('user');
  return userStr ? JSON.parse(userStr) : null;
}

// Helper: call Chengdu backend with stored JWT
async function chengduRequest(path: string, options?: RequestInit) {
  const token = localStorage.getItem('auth_token');
  const headers: Record<string, string> = {};
  if (token) headers['Authorization'] = `Bearer ${token}`;
  if (options?.method && options.method !== 'GET') headers['Content-Type'] = 'application/json';
  const url = `${CHENGDU_API}${path}`;
  console.log('[chengduRequest]', url);
  const res = await fetch(url, { ...options, headers: { ...headers, ...(options?.headers as Record<string, string> || {}) } });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    console.error('[chengduRequest] Failed:', res.status, text.slice(0, 200));
    throw new Error(`Chengdu ${path} failed: ${res.status}`);
  }
  return res.json();
}

export async function getUserProfile() {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    try {
      const data = await chengduRequest('/user/profile');
      // Update local cache
      if (data.user || data) {
        const user = data.user || data;
        localStorage.setItem('user', JSON.stringify(user));
      }
      return data;
    } catch (e) {
      // Fallback to cached
      const userStr = localStorage.getItem('user');
      return { user: userStr ? JSON.parse(userStr) : {} };
    }
  }
  const userStr = localStorage.getItem('user');
  return { user: userStr ? JSON.parse(userStr) : {} };
}

export async function updateUserProfile(data: Record<string, any>) {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    const token = localStorage.getItem('auth_token');
    const res = await fetch(`${CHENGDU_API}/user/profile`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
      body: JSON.stringify(data),
    });
    const result = await res.json();
    const userStr = localStorage.getItem('user');
    const user = userStr ? JSON.parse(userStr) : {};
    localStorage.setItem('user', JSON.stringify({ ...user, ...result }));
    return result;
  }
  const userStr = localStorage.getItem('user');
  const user = userStr ? JSON.parse(userStr) : {};
  const updated = { ...user, ...data };
  localStorage.setItem('user', JSON.stringify(updated));
  return updated;
}

export async function getUserUsage() {
  let usage: any = null;

  // Get plan info from Chengdu backend (requires auth_token from session-based login)
  if (isTauriApp && localStorage.getItem('auth_token')) {
    try {
      usage = await chengduRequest('/user/usage');
    } catch (_) {}
  }

  // In Electron mode, overlay gateway usage (the real usage data) onto Chengdu's plan info
  if (isTauriApp) {
    try {
      const gwUsage = await getGatewayUsage();
      if (gwUsage) {
        if (usage && usage.quota) {
          // Both sources available: combine
          if (usage.quota.window) {
            usage.quota.window.used = (usage.quota.window.used || 0) + (gwUsage.window_used || 0);
          }
          if (usage.quota.week) {
            usage.quota.week.used = (usage.quota.week.used || 0) + (gwUsage.week_used || 0);
          }
        } else if (!usage) {
          // No Chengdu auth_token — use gateway usage as primary source.
          // SG gateway's /gateway/usage calls Chengdu internal /user/:id/summary,
          // so it has the real plan+quota data even without a session cookie.
          usage = gwUsage;
        }
      }
    } catch (_) {}
  }

  if (usage) return usage;

  // selfhosted mode (no gateway, no Chengdu) — unlimited placeholder
  return {
    plan: { id: 999, name: 'Self-hosted', status: 'active', price: 0 },
    token_quota: 0,
    token_remaining: 0,
    used: 0,
    reset_date: '',
    is_unlimited: true
  };
}

export async function getUnreadAnnouncements() {
  const res = await request('/user/announcements');
  return res.json();
}

export async function markAnnouncementRead(id: number) {
  const res = await request(`/user/announcements/${id}/read`, {
    method: 'POST',
  });
  return res.json();
}

export async function getUserModels() {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    try { return await chengduRequest('/user/models'); } catch (_) {}
  }
  try {
    const res = await request('/user/models');
    return res.json();
  } catch (_) {
    return { all: [] };
  }
}

export async function getSessions() {
  const res = await request('/user/sessions');
  return res.json();
}

export async function deleteSession(id: string) {
  const res = await request(`/user/sessions/${id}`, { method: 'DELETE' });
  return res.json();
}

export async function logoutOtherSessions() {
  const res = await request('/user/sessions/logout-others', { method: 'POST' });
  return res.json();
}

export async function changePassword(currentPassword: string, newPassword: string) {
  const res = await request('/user/change-password', {
    method: 'POST',
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  });
  return res.json();
}

export async function deleteAccount(password: string) {
  const res = await request('/user/delete-account', {
    method: 'POST',
    body: JSON.stringify({ password }),
  });
  return res.json();
}

// 套餐与支付
export async function getPlans() {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    try { return await chengduRequest('/payment/plans'); } catch (_) {}
  }
  const res = await request('/payment/plans');
  return res.json();
}

export async function createPaymentOrder(planId: number, paymentMethod: string) {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    const token = localStorage.getItem('auth_token');
    const res = await fetch(`${CHENGDU_API}/payment/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
      body: JSON.stringify({ plan_id: planId, payment_method: paymentMethod }),
    });
    return res.json();
  }
  const res = await request('/payment/create', {
    method: 'POST',
    body: JSON.stringify({ plan_id: planId, payment_method: paymentMethod }),
  });
  return res.json();
}

export async function getPaymentStatus(orderId: string) {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    try { return await chengduRequest(`/payment/status/${orderId}`); } catch (_) {}
  }
  const res = await request(`/payment/status/${orderId}`);
  return res.json();
}

// 兑换码
export async function redeemCode(code: string) {
  if (isTauriApp && localStorage.getItem('auth_token')) {
    const token = localStorage.getItem('auth_token');
    const res = await fetch(`${CHENGDU_API}/redemption/redeem`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token}` },
      body: JSON.stringify({ code }),
    });
    return res.json();
  }
  const res = await request('/redemption/redeem', {
    method: 'POST',
    body: JSON.stringify({ code }),
  });
  return res.json();
}

// ═══ Projects ═══

export interface Project {
  id: string;
  name: string;
  description: string;
  instructions: string;
  workspace_path: string;
  is_archived: number;
  file_count?: number;
  chat_count?: number;
  created_at: string;
  updated_at: string;
}

export interface ProjectFile {
  id: string;
  project_id: string;
  file_name: string;
  file_path: string;
  file_size: number;
  mime_type: string;
  created_at: string;
}

export async function getProjects(): Promise<Project[]> {
  const res = await request('/projects');
  const data = await res.json();
  return Array.isArray(data) ? data : (Array.isArray(data?.projects) ? data.projects : []);
}

export async function createProject(name: string, description?: string): Promise<Project> {
  const res = await request('/projects', {
    method: 'POST',
    body: JSON.stringify({ name, description: description || '' }),
  });
  return res.json();
}

export async function getProject(id: string) {
  const res = await request(`/projects/${id}`);
  return res.json();
}

export async function updateProject(id: string, data: Partial<Pick<Project, 'name' | 'description' | 'instructions' | 'is_archived'>>) {
  const res = await request(`/projects/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function deleteProject(id: string) {
  const res = await request(`/projects/${id}`, { method: 'DELETE' });
  return res.json();
}

export async function uploadProjectFile(projectId: string, file: File): Promise<ProjectFile> {
  const formData = new FormData();
  formData.append('file', file);
  const token = getToken();
  const res = await fetch(`${API_BASE}/projects/${projectId}/files`, {
    method: 'POST',
    headers: token ? { 'Authorization': `Bearer ${token}` } : {},
    body: formData,
  });
  if (!res.ok) throw new Error('Upload failed');
  return res.json();
}

export async function deleteProjectFile(projectId: string, fileId: string) {
  const res = await request(`/projects/${projectId}/files/${fileId}`, { method: 'DELETE' });
  return res.json();
}

export async function getProjectConversations(projectId: string) {
  const res = await request(`/projects/${projectId}/conversations`);
  return res.json();
}

export async function createProjectConversation(projectId: string, title?: string, model?: string) {
  const res = await request(`/projects/${projectId}/conversations`, {
    method: 'POST',
    body: JSON.stringify({ title, model }),
  });
  return res.json();
}

// 对话相关
export async function getConversations() {
  if (isTauriApp) {
    await ensureNativeEngine();
    const { nativeEngineAPI } = await import('./utils/tauriAPI');
    const convs = await nativeEngineAPI.listConversations();
    return convs.map((c: any) => ({
      id: c.id,
      title: c.title,
      model: c.model,
      workspace_path: c.workspace_path,
      created_at: c.created_at,
      updated_at: c.updated_at,
      messages: [],
    }));
  }
  const res = await request('/conversations');
  const data = await res.json();
  return Array.isArray(data) ? data : (Array.isArray(data?.conversations) ? data.conversations : []);
}

export async function getUserArtifacts() {
  const res = await request('/artifacts');
  return res.json();
}

export async function getArtifactContent(filePath: string) {
  const res = await request('/artifacts/content?path=' + encodeURIComponent(filePath));
  return res.json();
}

export async function createConversation(title?: string, model?: string, extras?: { research_mode?: boolean }) {
  if (isTauriApp) {
    await ensureNativeEngine();
    const { nativeEngineAPI } = await import('./utils/tauriAPI');
    return nativeEngineAPI.createConversation({
      model: model || 'claude-sonnet-4-6',
      title,
      research_mode: extras?.research_mode,
    });
  }
  const body: any = { model };
  if (title !== undefined) {
    body.title = title;
  }
  if (extras?.research_mode !== undefined) {
    body.research_mode = extras.research_mode;
  }
  const res = await request('/conversations', {
    method: 'POST',
    body: JSON.stringify(body),
  });
  return res.json();
}

export async function getConversation(id: string) {
  if (isTauriApp) {
    await ensureNativeEngine();
    const { nativeEngineAPI } = await import('./utils/tauriAPI');
    const convs = await nativeEngineAPI.listConversations();
    const found = convs.find((c: any) => c.id === id);
    if (found) {
      const messages = await nativeEngineAPI.getMessages(id);
      return {
        id: found.id,
        title: found.title,
        model: found.model,
        workspace_path: found.workspace_path,
        created_at: found.created_at,
        updated_at: found.updated_at,
        messages: messages.map((m: any) => ({
          id: m.id,
          role: m.role,
          content: m.content,
          created_at: m.created_at,
          toolCalls: m.tool_calls,
        })),
      };
    }
    throw new Error('Conversation not found');
  }
  const res = await request(`/conversations/${id}`);
  return res.json();
}

export async function deleteConversation(id: string) {
  if (isTauriApp) {
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('conversationDeleting', { detail: { id } }));
    }
    await ensureNativeEngine();
    const { nativeEngineAPI } = await import('./utils/tauriAPI');
    await nativeEngineAPI.deleteConversation(id);
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('conversationDeleted', { detail: { id } }));
    }
    return { success: true };
  }

  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent('conversationDeleting', { detail: { id } }));
  }

  try {
    await request(`/conversations/${id}/stop-generation`, { method: 'POST' });
  } catch { }

  try {
    const res = await request(`/conversations/${id}`, { method: 'DELETE' });
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('conversationDeleted', { detail: { id } }));
    }
    return res.json();
  } catch (err) {
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('conversationDeleteFailed', { detail: { id } }));
    }
    throw err;
  }
}

export async function updateConversation(id: string, data: any) {
  if (isTauriApp) {
    return { ...data, id };
  }
  const res = await request(`/conversations/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function exportConversation(id: string): Promise<void> {
  const token = getToken();

  // Desktop (Electron) Logic
  if (typeof window !== 'undefined' && (window as any).electronAPI) {
    try {
      const conv = await getConversation(id);

      // Build a simple markdown snapshot
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

  // Web Fallback Logic
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


// 查询对话的活跃生成状态
export async function getGenerationStatus(conversationId: string) {
  const res = await request(`/conversations/${conversationId}/generation-status`);
  return res.json();
}

// 主动停止后台生成
export async function stopGeneration(conversationId: string) {
  const res = await request(`/conversations/${conversationId}/stop-generation`, { method: 'POST' });
  return res.json();
}

// 获取对话上下文大小
export async function getContextSize(conversationId: string): Promise<{ tokens: number; limit: number }> {
  const res = await request(`/conversations/${conversationId}/context-size`);
  return res.json();
}

// 手动压缩对话 — delegates to engine's /compact command
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

// 回答 AskUserQuestion — write control_response to engine stdin
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

// Pre-warm engine for a conversation (spawn in background before user sends first message)
export function warmEngine(conversationId: string): void {
  const userMode = getUserModeForConversation(conversationId);
  let userProfile: any;
  try {
    const p = JSON.parse(localStorage.getItem('user_profile') || localStorage.getItem('user') || '{}');
    const wf = p.work_function; const pp = p.personal_preferences;
    userProfile = (wf || pp) ? { work_function: wf, personal_preferences: pp } : undefined;
  } catch { userProfile = undefined; }

  let permissionMode: string | undefined;
  try {
    // Try window.__chatStore first (if available)
    if (typeof window !== 'undefined' && (window as any).__chatStore) {
      permissionMode = (window as any).__chatStore.getState().permissionMode;
    } else {
      // Fallback to localStorage persistence
      permissionMode = localStorage.getItem('permission_mode') || undefined;
    }
  } catch {}

  request(`/conversations/${conversationId}/warm`, {
    method: 'POST',
    body: JSON.stringify({
      ...resolveEnvCreds(userMode),
      user_mode: userMode,
      user_profile: userProfile,
      permission_mode: permissionMode,
    }),
  }).catch(() => {});
}

// ===== Provider Management =====
export interface ProviderModel { id: string; name: string; enabled?: boolean; }
export interface Provider {
  id: string; name: string; apiKey: string; baseUrl: string;
  format: 'anthropic' | 'openai'; models: ProviderModel[]; enabled: boolean;
  icon?: string;
  supportsWebSearch?: boolean;
  webSearchStrategy?: 'dashscope' | 'bigmodel' | 'anthropic_native' | null;
  webSearchTestedAt?: number;
  webSearchTestReason?: string | null;
}

export interface WebSearchTestResult {
  ok: boolean;
  strategy?: 'dashscope' | 'bigmodel' | 'anthropic_native' | null;
  hitCount?: number;
  reason?: string;
}

export async function testProviderWebSearch(id: string): Promise<WebSearchTestResult> {
  const res = await fetch(`${API_BASE}/providers/${id}/test-websearch`, { method: 'POST' });
  if (!res.ok) return { ok: false, reason: 'HTTP ' + res.status };
  return res.json();
}

export async function getProviders(): Promise<Provider[]> {
  const res = await fetch(`${API_BASE}/providers`);
  const data = await res.json();
  return Array.isArray(data) ? data : (Array.isArray(data?.providers) ? data.providers : []);
}

export async function createProvider(p: Partial<Provider>): Promise<Provider> {
  const res = await fetch(`${API_BASE}/providers`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(p) });
  return res.json();
}

export async function updateProvider(id: string, p: Partial<Provider>): Promise<Provider> {
  const res = await fetch(`${API_BASE}/providers/${id}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(p) });
  return res.json();
}

export async function deleteProvider(id: string): Promise<void> {
  await fetch(`${API_BASE}/providers/${id}`, { method: 'DELETE' });
}

export async function getProviderModels(): Promise<Array<{ id: string; name: string; providerId: string; providerName: string }>> {
  const res = await fetch(`${API_BASE}/providers/models`);
  return res.json();
}

// Check if a conversation has an active engine stream
export async function getStreamStatus(conversationId: string): Promise<{ active: boolean; eventCount: number }> {
  const res = await request(`/conversations/${conversationId}/stream-status`);
  return res.json();
}

// Reconnect to an active stream — receives buffered + live SSE events
export function reconnectStream(
  conversationId: string,
  onDelta: (delta: string, full: string) => void,
  onDone: (full: string) => void,
  onError: (err: string) => void,
  onThinking?: (thinking: string, full: string) => void,
  onSystem?: (event: string, message: string, data: any) => void,
  onToolUse?: (event: { type: 'start' | 'input' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; content?: string; is_error?: boolean; textBefore?: string }) => void,
  signal?: AbortSignal
): void {
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
            // Research mode events on reconnect path
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

// 删除指定消息及其后续消息
export async function deleteMessagesFrom(
  conversationId: string,
  messageId: string,
  preserveAttachmentIds?: string[]
) {
  const res = await request(`/conversations/${conversationId}/messages/${messageId}`, {
    method: 'DELETE',
    body: preserveAttachmentIds && preserveAttachmentIds.length > 0
      ? JSON.stringify({ preserve_attachment_ids: preserveAttachmentIds })
      : undefined,
  });
  return res.json();
}

// 删除对话末尾 N 条消息（编辑时 msg.id 不可用的回退方案）
export async function deleteMessagesTail(
  conversationId: string,
  count: number,
  preserveAttachmentIds?: string[]
) {
  const res = await request(`/conversations/${conversationId}/messages-tail/${count}`, {
    method: 'DELETE',
    body: preserveAttachmentIds && preserveAttachmentIds.length > 0
      ? JSON.stringify({ preserve_attachment_ids: preserveAttachmentIds })
      : undefined,
  });
  return res.json();
}

// 文件上传相关
export interface UploadResult {
  fileId: string;
  fileName: string;
  fileType: 'image' | 'document' | 'text';
  mimeType: string;
  size: number;
}

export async function uploadFile(
  file: File,
  onProgress?: (percent: number) => void,
  conversationId?: string
): Promise<UploadResult> {
  const port = await detectBridgePort();
  const uploadUrl = `http://127.0.0.1:${port}/api/upload`;
  
  return new Promise((resolve, reject) => {
    const token = getToken();
    const xhr = new XMLHttpRequest();
    const formData = new FormData();
    formData.append('file', file);

    xhr.upload.addEventListener('progress', (e) => {
      if (e.lengthComputable && onProgress) {
        onProgress(Math.round((e.loaded / e.total) * 100));
      }
    });

    xhr.addEventListener('load', () => {
      if (xhr.status === 401) {
        localStorage.removeItem('auth_token');
        localStorage.removeItem('user');
        window.location.hash = '#/login'; window.location.reload();
        reject(new Error('认证失效'));
        return;
      }
      const raw = xhr.responseText || '';
      let data: any = null;
      if (raw) {
        try {
          data = JSON.parse(raw);
        } catch {
          data = null;
        }
      }

      if (xhr.status >= 200 && xhr.status < 300) {
        if (data) {
          resolve(data);
          return;
        }
        reject(new Error('上传失败：服务器返回异常'));
        return;
      }

      const serverError = data?.error || data?.message;
      const rawError = !data && raw ? raw.slice(0, 120) : '';
      const detail = serverError || rawError || '上传失败';
      reject(new Error(`${detail} (HTTP ${xhr.status})`));
    });

    xhr.addEventListener('error', (err) => {
      console.error('[API] Upload network error:', err);
      reject(new Error(`网络错误，无法连接到 ${uploadUrl}`));
    });
    xhr.addEventListener('abort', () => reject(new Error('上传已取消')));

    xhr.open('POST', uploadUrl);
    if (token) {
      xhr.setRequestHeader('Authorization', `Bearer ${token}`);
    }
    if (conversationId) {
      xhr.setRequestHeader('x-conversation-id', conversationId);
    }
    xhr.send(formData);
  });
}

export async function deleteAttachment(fileId: string): Promise<void> {
  await request(`/uploads/${fileId}`, { method: 'DELETE' });
}

export function getAttachmentUrl(fileId: string): string {
  return `${API_BASE}/uploads/${fileId}/raw`;
}

// Skills 相关
export async function getSkills() {
  const res = await request('/skills');
  return res.json();
}

export async function getSkillDetail(id: string) {
  const res = await request(`/skills/${id}`);
  return res.json();
}

export async function getSkillFile(id: string, filePath: string) {
  const res = await request(`/skills/${id}/file?path=${encodeURIComponent(filePath)}`);
  return res.json();
}

export async function createSkill(data: { name: string; description?: string; content?: string }) {
  const res = await request('/skills', {
    method: 'POST',
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function updateSkill(id: string, data: { name?: string; description?: string; content?: string }) {
  const res = await request(`/skills/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function deleteSkill(id: string) {
  const res = await request(`/skills/${id}`, { method: 'DELETE' });
  return res.json();
}

export async function toggleSkill(id: string, enabled: boolean) {
  const res = await request(`/skills/${id}/toggle`, {
    method: 'PATCH',
    body: JSON.stringify({ enabled }),
  });
  return res.json();
}

// GitHub Connector
export async function getGithubStatus() {
  const res = await fetch(`${API_BASE}/github/status`);
  return res.json();
}

export async function getGithubAuthUrl() {
  const res = await fetch(`${API_BASE}/github/auth-url`);
  return res.json();
}

export async function disconnectGithub() {
  const res = await fetch(`${API_BASE}/github/disconnect`, { method: 'POST' });
  return res.json();
}

export async function getGithubRepos(page = 1) {
  const res = await fetch(`${API_BASE}/github/repos?page=${page}`);
  return res.json();
}

export async function getGithubTree(owner: string, repo: string, ref = '') {
  const qs = ref ? `?ref=${encodeURIComponent(ref)}` : '';
  const res = await fetch(`${API_BASE}/github/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/tree${qs}`);
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to fetch tree' }));
    throw new Error(err.error || 'Failed to fetch tree');
  }
  return res.json();
}

export async function getGithubContents(owner: string, repo: string, path = '', ref = '') {
  const params = new URLSearchParams();
  if (path) params.set('path', path);
  if (ref) params.set('ref', ref);
  const qs = params.toString();
  const url = `${API_BASE}/github/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/contents${qs ? '?' + qs : ''}`;
  const res = await fetch(url);
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to fetch contents' }));
    throw new Error(err.error || 'Failed to fetch contents');
  }
  return res.json();
}

export async function materializeGithub(
  conversationId: string,
  repoFullName: string,
  ref: string,
  selections: Array<{ path: string; isFolder: boolean }>
): Promise<{ ok: boolean; repoFullName: string; ref: string; rootDir: string; fileCount: number; skipped: number }> {
  const res = await fetch(`${API_BASE}/github/materialize`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ conversationId, repoFullName, ref, selections }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Materialize failed' }));
    throw new Error(err.error || 'Materialize failed');
  }
  return res.json();
}

// 检查模型是否需要使用前端代理（当 Provider 使用 OpenAI 格式时）
async function checkUseProxyForModel(model: string): Promise<boolean> {
  try {
    const providers = await getProviders();
    for (const p of providers) {
      if (!p.enabled) continue;
      const hasModel = p.models?.some((m: any) => m.id === model && m.enabled !== false);
      if (hasModel && p.format === 'openai') {
        console.log(`[API] Using frontend proxy for model "${model}" (provider: ${p.name}, format: openai)`);
        return true;
      }
    }
  } catch (e) {
    console.warn('[API] Failed to check providers for proxy:', e);
  }
  return false;
}

// 通过前端代理发送消息（支持 OpenAI 格式的 Provider）
async function sendMessageViaProxy(
  conversationId: string,
  messages: any[],
  model: string,
  onDelta: (delta: string, full: string) => void,
  onDone: (full: string) => void,
  onError: (err: string) => void,
  onThinking?: (thinking: string, full: string) => void,
  onSystem?: (event: string, message: string, data: any) => void,
  onToolUse?: (event: { type: 'start' | 'input' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; content?: string; is_error?: boolean; textBefore?: string }) => void,
): Promise<() => void> {
  const { apiProxy, resolveProviderForModel } = await import('./utils/apiProxy');
  const providers = await getProviders();
  
  // 转换为代理格式
  const proxyProviders = providers.map((p: any) => ({
    id: p.id,
    name: p.name,
    base_url: p.baseUrl,
    api_key: p.apiKey || '',
    api_format: p.format === 'openai' ? 'openai' as const : 'anthropic' as const,
    enabled: p.enabled,
    models: (p.models || []).map((m: any) => ({
      id: m.id,
      name: m.name,
      enabled: m.enabled,
    })),
  }));

  apiProxy.setProviders(proxyProviders);
  
  const anthropicMessages = messages.map((msg: any) => {
    if (msg.role === 'user') {
      return { role: 'user', content: msg.content };
    } else if (msg.role === 'assistant') {
      return { role: 'assistant', content: msg.content };
    }
    return msg;
  });

  const request = {
    model,
    messages: anthropicMessages,
    max_tokens: 8192,
    stream: true,
  };

  let fullText = '';
  let thinkingText = '';
  let currentToolUseId: string | null = null;
  let currentToolName: string | undefined = undefined;

  const streamHandlers = {
    onDelta: (delta: string, full: string) => {
      fullText = full;
      onDelta(delta, fullText);
    },
    onThinking: (delta: string, full: string) => {
      thinkingText = full;
      onThinking?.(delta, thinkingText);
    },
    onToolUse: (event: { type: 'start' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; output?: string; is_error?: boolean }) => {
      if (event.type === 'start') {
        currentToolUseId = event.tool_use_id;
        currentToolName = event.tool_name;
        onToolUse?.({ type: 'start', tool_use_id: event.tool_use_id, tool_name: event.tool_name, tool_input: event.tool_input, content: '', is_error: false });
      } else if (event.type === 'done') {
        onToolUse?.({ type: 'done', tool_use_id: event.tool_use_id, tool_name: currentToolName, tool_input: {}, content: event.output, is_error: event.is_error || false });
        currentToolUseId = null;
        currentToolName = undefined;
      }
    },
    onSystem: (event: string, data: any) => {
      onSystem?.(event, '', data);
    },
    onDone: (full: string) => {
      fullText = full;
      onDone(fullText);
    },
    onError: (err: string) => {
      onError(err);
    },
  };

  try {
    const stream = await apiProxy.chatStream(request);
    parseProxyStream(stream, streamHandlers);
  } catch (e: any) {
    onError(e.message || 'Failed to send message via proxy');
  }

  return () => {
    // 清理函数
  };
}

// 解析代理返回的 SSE 流
async function parseProxyStream(stream: ReadableStream, handlers: {
  onDelta: (delta: string, full: string) => void;
  onThinking?: (delta: string, full: string) => void;
  onToolUse?: (event: { type: 'start' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; output?: string; is_error?: boolean }) => void;
  onSystem?: (event: string, data: any) => void;
  onDone: (full: string) => void;
  onError: (err: string) => void;
}): Promise<void> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let fullText = '';
  let thinkingText = '';
  let currentToolUseId: string | null = null;
  let currentToolName: string | undefined = undefined;

  const processLine = (line: string) => {
    if (!line.startsWith('data:')) return;
    const data = line.startsWith('data: ') ? line.slice(6) : line.slice(5);
    if (data === '[DONE]') {
      handlers.onDone(fullText);
      return;
    }

    try {
      const event = JSON.parse(data);
      const eventType = event.type;

      switch (eventType) {
        case 'message_start':
          handlers.onSystem?.('message_start', { model: event.message?.model });
          break;

        case 'content_block_start':
          if (event.content_block?.type === 'tool_use') {
            currentToolUseId = event.content_block.id;
            currentToolName = event.content_block.name || undefined;
            handlers.onToolUse?.({
              type: 'start',
              tool_use_id: currentToolUseId || '',
              tool_name: currentToolName,
              tool_input: {},
            });
          }
          break;

        case 'content_block_delta':
          if (event.delta?.type === 'text_delta' && event.delta.text) {
            fullText += event.delta.text;
            handlers.onDelta(event.delta.text, fullText);
          } else if (event.delta?.type === 'thinking_delta' && event.delta.thinking) {
            thinkingText += event.delta.thinking;
            handlers.onThinking?.(event.delta.thinking, thinkingText);
          }
          break;

        case 'content_block_stop':
          break;

        case 'message_delta':
          if (event.delta?.stop_reason) {
            handlers.onSystem?.('message_delta', { stop_reason: event.delta.stop_reason });
          }
          break;

        case 'message_stop':
          handlers.onDone(fullText);
          break;

        case 'error':
          handlers.onError(event.error || 'Unknown error');
          break;
      }
    } catch (e) {
      // Skip malformed JSON
    }
  };

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        if (buffer) {
          for (const line of buffer.split('\n')) {
            processLine(line);
          }
        }
        if (!fullText && !thinkingText) {
          handlers.onDone('');
        }
        break;
      }

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        processLine(line);
      }
    }
  } catch (e: any) {
    handlers.onError(`Stream error: ${e.message}`);
  }
}

// 流式对话（核心 - Tauri 版本，直接使用 bridge-server HTTP API）
export async function sendMessageNative(
  conversationId: string,
  messages: any[],
  model: string,
  onDelta: (delta: string, full: string) => void,
  onDone: (full: string) => void,
  onError: (err: string) => void,
  onThinking?: (thinking: string, full: string) => void,
  onSystem?: (event: string, message: string, data: any) => void,
  onToolUse?: (event: { type: 'start' | 'input' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; content?: string; is_error?: boolean; textBefore?: string }) => void,
): Promise<() => void> {
  const token = getToken();
  let fullText = '';
  let thinkingText = '';
  let deltaCount = 0;

  console.log(`[API] Sending message (native): model=${model}, messages=${messages.length}, stream=true`);
  console.log(`[API] Request URL: ${API_BASE}/chat`);
  console.log(`[API] Establishing SSE connection to ${API_BASE}/chat`);

  // Read permissionMode from localStorage (persisted by useChatStore.setPermissionMode)
  let permissionMode: string | undefined;
  try {
    permissionMode = localStorage.getItem('permission_mode') || undefined;
  } catch {}

  try {
    await detectBridgePort();
    const res = await fetch(`${API_BASE}/chat`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({
        conversation_id: conversationId,
        messages,
        model,
        ...resolveEnvCreds(getUserModeForConversation(conversationId)),
        user_mode: getUserModeForConversation(conversationId),
        permission_mode: permissionMode,
      }),
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: '请求失败' }));
      onError(err.error || '请求失败');
      return () => {};
    }

    if (!res.body) return () => {};

    const reader = res.body.getReader();
    console.log(`[API] SSE connection established, reading stream...`);
    const decoder = new TextDecoder();
    let buffer = '';
    let pendingTextDelta = '';
    let pendingThinkingDelta = '';
    let flushScheduled = false;

    const flushPending = () => {
      flushScheduled = false;
      if (pendingThinkingDelta && onThinking) {
        const delta = pendingThinkingDelta;
        pendingThinkingDelta = '';
        onThinking(delta, thinkingText);
      }
      if (pendingTextDelta) {
        const delta = pendingTextDelta;
        pendingTextDelta = '';
        fullText += delta;
        onDelta(delta, fullText);
      }
    };

    const scheduleFlush = () => {
      if (!flushScheduled) {
        flushScheduled = true;
        setTimeout(flushPending, 0);
      }
    };

    const processLine = (line: string) => {
      if (!line.startsWith('event:')) {
        if (line.startsWith('data:')) {
          const data = line.startsWith('data: ') ? line.slice(6) : line.slice(5);
          if (data === '[DONE]') return;
          try {
            const event = JSON.parse(data);
            if (event.type === 'text' && event.text) {
              deltaCount++;
              if (deltaCount % 50 === 0) {
                console.log(`[API] Stream progress: ${deltaCount} deltas received, ${fullText.length} chars`);
              }
              pendingTextDelta += event.text;
              scheduleFlush();
            } else if (event.type === 'thinking' && event.thinking) {
              thinkingText += event.thinking;
              pendingThinkingDelta += event.thinking;
              scheduleFlush();
            } else if (event.type === 'content_block_start') {
              if (event.content_block?.type === 'tool_use') {
                onToolUse?.({
                  type: 'start',
                  tool_use_id: event.content_block.id || '',
                  tool_name: event.content_block.name || '',
                  tool_input: event.content_block.input || {},
                  content: '',
                  is_error: false,
                });
              }
            } else if (event.type === 'content_block_delta') {
              if (event.delta?.type === 'tool_use_delta') {
                // Tool input streaming
              } else if (event.delta?.type === 'text_delta' && event.delta.text) {
                pendingTextDelta += event.delta.text;
                scheduleFlush();
              }
            } else if (event.type === 'content_block_stop') {
            } else if (event.type === 'tool_use_start') {
              onToolUse?.({
                type: 'start',
                tool_use_id: event.tool_use_id || '',
                tool_name: event.tool_name || '',
                tool_input: event.tool_input || {},
                content: '',
                is_error: false,
              });
            } else if (event.type === 'tool_use_done') {
              onToolUse?.({
                type: 'done',
                tool_use_id: event.tool_use_id || '',
                tool_name: event.tool_name || 'unknown',
                tool_input: event.tool_input || {},
                content: event.output || event.content || '',
                is_error: event.is_error === true,
              });
            } else if (event.type === 'tool_arg_delta') {
            } else if (event.type === 'message_start') {
              onSystem?.('message_start', '', { model: event.message?.model });
            } else if (event.type === 'message_delta') {
              if (event.delta?.stop_reason) {
                onSystem?.('message_delta', '', { stop_reason: event.delta.stop_reason });
              }
            } else if (event.type === 'message_stop') {
              flushPending();
              console.log(`[API] Stream complete: ${deltaCount} deltas, ${fullText.length} chars total`);
              console.log(`[API] SSE connection closed`);
              onDone(fullText);
            } else if (event.type === 'error') {
              console.log(`[API] SSE connection closed`);
              onError(event.error || 'Unknown error');
            }
          } catch (e) {
            // Skip malformed JSON
          }
        }
      }
    };

    const readChunk = async () => {
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) {
            if (buffer) {
              for (const line of buffer.split('\n')) {
                processLine(line);
              }
            }
            flushPending();
            if (!fullText && !thinkingText) {
              onDone('');
            }
            console.log(`[API] SSE connection closed`);
            break;
          }
          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split('\n');
          buffer = lines.pop() || '';
          for (const line of lines) {
            processLine(line);
          }
        }
      } catch (e: any) {
        console.log(`[API] SSE connection closed`);
        onError(`Stream error: ${e.message}`);
      }
    };

    readChunk();
  } catch (e: any) {
    console.log(`[API] SSE connection closed`);
    onError(e.message || 'Failed to send message');
  }

  return () => {
    // 清理函数
  };
}

// 流式对话（核心 - HTTP 版本）
export async function sendMessage(
  conversationId: string,
  message: string,
  attachments: any[] | null,
  onDelta: (delta: string, full: string) => void,
  onDone: (full: string) => void,
  onError: (err: string) => void,
  onThinking?: (thinking: string, full: string) => void,
  onSystem?: (event: string, message: string, data: any) => void,
  onCitations?: (citations: Array<{ url: string; title: string; cited_text?: string }>, query?: string, tokens?: number) => void,
  onDocument?: (document: { id: string; title: string; filename: string; url: string; content?: string; format?: 'markdown' | 'docx' | 'pptx'; slides?: Array<{ title: string; content: string; notes?: string }> }) => void,
  onDocumentDraft?: (draft: { draft_id: string; title?: string; format?: string; preview?: string; preview_available?: boolean; done?: boolean; document?: any }) => void,
  onCodeExecution?: (data: { type: string; executionId: string; code?: string; language?: string; files?: Array<{ id: string; name: string }>; stdout?: string; stderr?: string; images?: string[]; error?: string | null }) => void,
  onToolUse?: (event: { type: 'start' | 'input' | 'done'; tool_use_id: string; tool_name?: string; tool_input?: any; content?: string; is_error?: boolean; textBefore?: string }) => void,
  signal?: AbortSignal,
  model?: string,
  messages?: any[],
) {
  const token = getToken();
  let fullText = '';
  let deltaCount = 0;
  console.log(`[API] Sending message: model=${model}, messages=${messages?.length || 0}, stream=true`);
  console.log(`[API] Request URL: ${API_BASE}/chat`);
  console.log(`[API] Establishing SSE connection to ${API_BASE}/chat`);
  try {
    if (isTauriApp) {
      await detectBridgePort();
    }
    // Read permissionMode from store (if available)
    let permissionMode: string | undefined;
    try {
      if (typeof window !== 'undefined' && (window as any).__chatStore) {
        permissionMode = (window as any).__chatStore.getState().permissionMode;
      }
    } catch {}
    const res = await fetch(`${API_BASE}/chat`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
      },
      body: JSON.stringify({
        conversation_id: conversationId,
        message,
        model: model || undefined,
        messages: messages && messages.length > 0 ? messages : undefined,
        attachments: attachments || undefined,
        ...resolveEnvCreds(getUserModeForConversation(conversationId)),
        user_mode: getUserModeForConversation(conversationId),
        permission_mode: permissionMode,
        user_profile: (() => {
          try {
            const p = JSON.parse(localStorage.getItem('user_profile') || localStorage.getItem('user') || '{}');
            const wf = p.work_function;
            const pp = p.personal_preferences;
            return (wf || pp) ? { work_function: wf, personal_preferences: pp } : undefined;
          } catch { return undefined; }
        })(),
      }),
      signal,
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: '请求失败' }));
      onError(err.error || '请求失败');
      return;
    }

    if (!res.body) return;

    const reader = res.body.getReader();
    console.log(`[API] SSE connection established, reading stream...`);
    const decoder = new TextDecoder();
    let buffer = '';
    let thinkingText = '';
    let pendingTextDelta = '';
    let pendingThinkingDelta = '';
    let flushScheduled = false;
    const INLINE_ARTIFACT_OPEN = '<cp_artifact';
    const INLINE_ARTIFACT_CLOSE = '</cp_artifact>';
    let inlineArtifactBuffer = '';
    let inlineArtifactSeq = 0;
    let activeInlineArtifact: null | {
      draft_id: string;
      title: string;
      format: string;
      preview: string;
    } = null;

    const flushPending = () => {
      flushScheduled = false;
      if (pendingThinkingDelta && onThinking) {
        const delta = pendingThinkingDelta;
        pendingThinkingDelta = '';
        onThinking(delta, thinkingText);
      }
      if (pendingTextDelta) {
        const delta = pendingTextDelta;
        pendingTextDelta = '';
        onDelta(delta, fullText);
      }
    };

    const scheduleFlush = () => {
      if (flushScheduled) return;
      flushScheduled = true;
      if (typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function') {
        window.requestAnimationFrame(() => flushPending());
      } else {
        setTimeout(flushPending, 16);
      }
    };

    const appendVisibleText = (text: string) => {
      if (!text) return;
      fullText += text;
      pendingTextDelta += text;
      scheduleFlush();
    };

    const emitInlineArtifactDraft = (done = false) => {
      if (!activeInlineArtifact || !onDocumentDraft) return;
      onDocumentDraft({
        draft_id: activeInlineArtifact.draft_id,
        title: activeInlineArtifact.title,
        format: activeInlineArtifact.format,
        preview: activeInlineArtifact.preview,
        preview_available: activeInlineArtifact.preview.length > 0,
        done,
      });
    };

    const appendInlineArtifactPreview = (text: string) => {
      if (!text || !activeInlineArtifact) return;
      activeInlineArtifact.preview += text;
      emitInlineArtifactDraft(false);
    };

    const parseInlineArtifactAttrs = (tagText: string) => {
      const titleMatch = tagText.match(/title="([^"]*)"/i);
      const formatMatch = tagText.match(/format="([^"]*)"/i);
      return {
        title: (titleMatch?.[1] || '').trim() || 'Untitled document',
        format: (formatMatch?.[1] || 'markdown').trim() || 'markdown',
      };
    };

    const processInlineArtifactText = (chunk: string, flushAll = false) => {
      if (!chunk && !flushAll) return;
      inlineArtifactBuffer += chunk;

      while (inlineArtifactBuffer) {
        if (!activeInlineArtifact) {
          const startIdx = inlineArtifactBuffer.indexOf(INLINE_ARTIFACT_OPEN);
          if (startIdx === -1) {
            if (flushAll) {
              appendVisibleText(inlineArtifactBuffer);
              inlineArtifactBuffer = '';
            } else {
              const keep = Math.min(inlineArtifactBuffer.length, INLINE_ARTIFACT_OPEN.length - 1);
              const emit = inlineArtifactBuffer.slice(0, inlineArtifactBuffer.length - keep);
              if (emit) appendVisibleText(emit);
              inlineArtifactBuffer = inlineArtifactBuffer.slice(inlineArtifactBuffer.length - keep);
            }
            break;
          }

          if (startIdx > 0) {
            appendVisibleText(inlineArtifactBuffer.slice(0, startIdx));
            inlineArtifactBuffer = inlineArtifactBuffer.slice(startIdx);
          }

          const tagEndIdx = inlineArtifactBuffer.indexOf('>');
          if (tagEndIdx === -1) {
            if (flushAll) {
              appendVisibleText(inlineArtifactBuffer);
              inlineArtifactBuffer = '';
            }
            break;
          }

          const tagText = inlineArtifactBuffer.slice(0, tagEndIdx + 1);
          const attrs = parseInlineArtifactAttrs(tagText);
          inlineArtifactSeq += 1;
          activeInlineArtifact = {
            draft_id: `inline-artifact-${inlineArtifactSeq}`,
            title: attrs.title,
            format: attrs.format,
            preview: '',
          };
          emitInlineArtifactDraft(false);
          inlineArtifactBuffer = inlineArtifactBuffer.slice(tagEndIdx + 1);
          continue;
        }

        const closeIdx = inlineArtifactBuffer.indexOf(INLINE_ARTIFACT_CLOSE);
        if (closeIdx === -1) {
          if (flushAll) {
            appendInlineArtifactPreview(inlineArtifactBuffer);
            inlineArtifactBuffer = '';
            emitInlineArtifactDraft(true);
            activeInlineArtifact = null;
          } else {
            const keep = Math.min(inlineArtifactBuffer.length, INLINE_ARTIFACT_CLOSE.length - 1);
            const emit = inlineArtifactBuffer.slice(0, inlineArtifactBuffer.length - keep);
            if (emit) appendInlineArtifactPreview(emit);
            inlineArtifactBuffer = inlineArtifactBuffer.slice(inlineArtifactBuffer.length - keep);
          }
          break;
        }

        if (closeIdx > 0) {
          appendInlineArtifactPreview(inlineArtifactBuffer.slice(0, closeIdx));
        }
        inlineArtifactBuffer = inlineArtifactBuffer.slice(closeIdx + INLINE_ARTIFACT_CLOSE.length);
        emitInlineArtifactDraft(true);
        activeInlineArtifact = null;
      }
    };

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || ''; // 保留不完整的最后一行

      for (const line of lines) {
        if (!line.startsWith('data:')) continue;
        const data = line.startsWith('data: ') ? line.slice(6) : line.slice(5);
        if (data.trim() === '[DONE]') {
          processInlineArtifactText('', true);
          flushPending();
          console.log(`[API] Stream complete: ${deltaCount} deltas, ${fullText.length} chars total`);
          console.log(`[API] SSE connection closed`);
          onDone(fullText);
          return;
        }

        try {
          const parsed = JSON.parse(data);

          // 处理 system 事件（如 compaction 通知）
          if (parsed.type === 'system') {
            if (onSystem) {
              onSystem(parsed.event, parsed.message, parsed);
            }
            continue;
          }

          // 处理 status 事件（如搜索状态通知）
          if (parsed.type === 'status') {
            if (onSystem) {
              onSystem('status', parsed.message, parsed);
            }
            continue;
          }

          if (parsed.type === 'thinking_summary' && parsed.summary) {
            if (onSystem) {
              onSystem('thinking_summary', parsed.summary, parsed);
            }
            continue;
          }

          // 处理搜索来源事件
          if (parsed.type === 'search_sources') {
            if (onCitations && Array.isArray(parsed.sources)) {
              onCitations(parsed.sources, parsed.query, parsed.tokens);
            }
            continue;
          }

          // 处理文档创建事件
          if (parsed.type === 'document_created') {
            if (onDocument && parsed.document) {
              onDocument(parsed.document);
            }
            continue;
          }

          // 处理文档更新事件
          if (parsed.type === 'document_updated') {
            if (onDocument && parsed.document) {
              onDocument(parsed.document);
            }
            continue;
          }

          if (parsed.type === 'document_draft') {
            if (onDocumentDraft) {
              onDocumentDraft(parsed);
            }
            continue;
          }

          // 处理代码执行事件
          if (parsed.type === 'code_execution') {
            if (onCodeExecution) {
              onCodeExecution(parsed);
            }
            continue;
          }

          // 处理代码执行结果事件
          if (parsed.type === 'code_result') {
            if (onCodeExecution) {
              onCodeExecution(parsed);
            }
            continue;
          }

          // 处理 thinking 内容
          if (parsed.type === 'content_block_delta' && parsed.delta) {
            if (parsed.delta.type === 'text_delta' && parsed.delta.text) {
              deltaCount++;
              if (deltaCount % 50 === 0) {
                console.log(`[API] Stream progress: ${deltaCount} deltas received, ${fullText.length} chars`);
              }
              const textChunk = parsed.delta.text;
              // 处理中转 API 将 <thinking> 标签嵌入 text 的情况
              if (textChunk.includes('<thinking>') || textChunk.includes('</thinking>')) {
                const thinkRegex = /<thinking>([\s\S]*?)<\/thinking>/g;
                let match;
                let cleaned = textChunk;
                while ((match = thinkRegex.exec(textChunk)) !== null) {
                  if (onThinking) {
                    thinkingText += match[1];
                    pendingThinkingDelta += match[1];
                    scheduleFlush();
                  }
                }
                cleaned = textChunk.replace(/<thinking>[\s\S]*?<\/thinking>\s*/g, '');
                if (cleaned) {
                  processInlineArtifactText(cleaned);
                }
              } else {
                processInlineArtifactText(textChunk);
              }
            }
            if (parsed.delta.type === 'thinking_delta' && parsed.delta.thinking) {
              thinkingText += parsed.delta.thinking;
              if (onThinking) {
                pendingThinkingDelta += parsed.delta.thinking;
                scheduleFlush();
              }
            }
          }

          // 处理 content_block_start 来识别 thinking block
          if (parsed.type === 'content_block_start' && parsed.content_block) {
            if (parsed.content_block.type === 'thinking' && onThinking) {
              // 新的 thinking block 开始
              thinkingText = '';
            }
          }

          // Handle compact_boundary from engine auto-compact
          if (parsed.type === 'compact_boundary') {
            if (onSystem) {
              onSystem('compact_boundary', '', parsed);
            }
            continue;
          }

          // Handle AskUserQuestion from engine
          if (parsed.type === 'ask_user') {
            if (onSystem) {
              onSystem('ask_user', '', parsed);
            }
            continue;
          }

          // Handle tool permission request from engine
          if (parsed.type === 'tool_permission') {
            if (onSystem) {
              onSystem('tool_permission', '', parsed);
            }
            continue;
          }

          // Handle message_start with usage data
          if (parsed.type === 'message_start') {
            if (onSystem) {
              onSystem('message_start', '', parsed);
            }
            continue;
          }

          // Handle message_delta with usage data
          if (parsed.type === 'message_delta') {
            if (onSystem) {
              onSystem('message_delta', '', parsed);
            }
            continue;
          }

          // Handle task/agent progress events
          if (parsed.type === 'task_event') {
            if (onSystem) {
              onSystem('task_event', '', parsed);
            }
            continue;
          }

          // Handle tool use events
          if (parsed.type === 'tool_use_start' && onToolUse) {
            onToolUse({ type: 'start', tool_use_id: parsed.tool_use_id, tool_name: parsed.tool_name, tool_input: parsed.tool_input, textBefore: parsed.textBefore || '' });
          }
          if (parsed.type === 'tool_use_input' && onToolUse) {
            onToolUse({ type: 'input', tool_use_id: parsed.tool_use_id, tool_input: parsed.tool_input });
          }
          if (parsed.type === 'tool_use_done' && onToolUse) {
            onToolUse({ type: 'done', tool_use_id: parsed.tool_use_id, content: parsed.content || parsed.output, is_error: parsed.is_error });
          }
          if (parsed.type === 'tool_arg_delta') {
            // Tool argument streaming delta — can be ignored or logged
          }

          // Research mode events — forward as system events for MainContent to handle
          if (parsed.type && parsed.type.startsWith('research_') && onSystem) {
            onSystem(parsed.type, '', parsed);
            // research_report_delta also feeds into the streaming text so the
            // final report appears as the assistant message body
            if (parsed.type === 'research_report_delta' && parsed.text) {
              fullText += parsed.text;
              onDelta(parsed.text, fullText);
            }
            continue;
          }

          // Track text offset where tool work ends and final response begins
          if (parsed.type === 'tool_text_offset' && onSystem) {
            onSystem('tool_text_offset', '', parsed);
          }

          if (parsed.type === 'message_stop') {
            processInlineArtifactText('', true);
            if (fullText) {
              flushPending();
              console.log(`[API] Stream complete: ${deltaCount} deltas, ${fullText.length} chars total`);
              console.log(`[API] SSE connection closed`);
              onDone(fullText);
              return;
            }
            continue;
          }

          if (parsed.type === 'error') {
            const detail = parsed.detail ? `\n${parsed.detail}` : '';
            processInlineArtifactText('', true);
            flushPending();
            console.log(`[API] SSE connection closed`);
            onError((parsed.error || '未知错误') + detail);
            return;
          }
        } catch (e) {
          // 忽略非JSON行
        }
      }
    }

    processInlineArtifactText('', true);
    if (fullText) {
      flushPending();
      console.log(`[API] SSE connection closed`);
      onDone(fullText);
    } else {
      flushPending();
      console.log(`[API] SSE connection closed`);
      onDone('');
    }
  } catch (err: any) {
    if (err.name === 'AbortError') {
      onDone(fullText);
      return;
    }
    console.log(`[API] SSE connection closed`);
    onError(err.message || 'Network error');
  }
}

// Code API 相关
export async function getCodeSSO() {
  const res = await request('/code/sso');
  return res.json();
}

export async function getCodeQuota() {
  const res = await request('/code/quota');
  return res.json();
}

export async function getCodePlans() {
  const res = await request('/code/plans');
  return res.json();
}

export async function createWorktree(data: { branch_prefix?: string; agent_name?: string; task?: string; model?: string }) {
  const res = await request('/worktrees', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function listWorktrees() {
  const res = await request('/worktrees');
  return res.json();
}

export async function getWorktree(id: string) {
  const res = await request(`/worktrees/${id}`);
  return res.json();
}

export async function removeWorktree(id: string) {
  const res = await request(`/worktrees/${id}`, { method: 'DELETE' });
  return res.json();
}

export async function mergeWorktree(worktreeId: string, strategy?: string) {
  const res = await request('/worktrees/merge', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ worktree_id: worktreeId, strategy }),
  });
  return res.json();
}

export async function syncWorktrees() {
  const res = await request('/worktrees/sync', { method: 'POST' });
  return res.json();
}

export async function listAgents() {
  const res = await request('/agents');
  return res.json();
}

export async function getAgent(id: string) {
  const res = await request(`/agents/${id}`);
  return res.json();
}

export async function cancelAgent(id: string) {
  const res = await request(`/agents/${id}/cancel`, { method: 'POST' });
  return res.json();
}

export async function getIdeStatus() {
  const res = await request('/ide/status');
  return res.json();
}

export async function startIdeServer() {
  const res = await request('/ide/start', { method: 'POST' });
  return res.json();
}

export async function stopIdeServer() {
  const res = await request('/ide/stop', { method: 'POST' });
  return res.json();
}

export async function getIdeConnections() {
  const res = await request('/ide/connections');
  return res.json();
}

export async function disconnectIde(id: string) {
  const res = await request(`/ide/connections/${id}`, { method: 'DELETE' });
  return res.json();
}

export async function trackEvent(eventType: string, properties?: Record<string, any>, sessionId?: string) {
  try {
    const res = await request('/analytics/track', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ event_type: eventType, properties, session_id: sessionId }),
    });
    return res.json();
  } catch (_) { return { success: false }; }
}

export async function getAnalyticsDaily(date: string) {
  const res = await request(`/analytics/daily/${date}`);
  return res.json();
}

export async function getAnalyticsRange(from: string, to: string) {
  const res = await request(`/analytics/range?from=${from}&to=${to}`);
  return res.json();
}

export async function getAnalyticsSummary(days = 30) {
  const res = await request(`/analytics/summary?days=${days}`);
  return res.json();
}

export async function getAnalyticsEventCounts(days = 30) {
  const res = await request(`/analytics/event-counts?days=${days}`);
  return res.json();
}

export async function getAnalyticsRecentEvents(limit = 50) {
  const res = await request(`/analytics/recent-events?limit=${limit}`);
  return res.json();
}

export async function multiagentResearch(
  query: string,
  model: string,
  onEvent: (event: any) => void
): Promise<void> {
  await detectBridgePort();
  const res = await fetch(`${API_BASE}/multiagent/research`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, model_id: model, user_id: 'default' })
  });

  if (!res.ok || !res.body) {
    throw new Error('Multiagent research request failed');
  }

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
      if (line.startsWith('data:')) {
        const data = line.slice(5).trim();
        if (data === '[DONE]') return;
        try {
          const event = JSON.parse(data);
          onEvent(event);
        } catch {}
      } else if (line.startsWith('event:')) {
        // SSE event type line, consumed with data line
      }
    }
  }
}

// ===== Terminal / PTY =====
export interface TerminalSession {
  id: string;
  shell: string;
  cwd: string;
  created_at: string;
}

export async function createTerminal(cwd?: string, shell?: string): Promise<{ terminal_id: string }> {
  const body: Record<string, string> = {};
  if (cwd) body.cwd = cwd;
  if (shell) body.shell = shell;
  const res = await fetch(`${API_BASE}/terminals`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to create terminal' }));
    throw new Error(err.error || 'Failed to create terminal');
  }
  return res.json();
}

export async function writeTerminal(terminalId: string, data: string): Promise<void> {
  const res = await fetch(`${API_BASE}/terminals/${encodeURIComponent(terminalId)}/write`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ data }),
  });
  if (!res.ok) throw new Error('Failed to write to terminal');
}

export async function resizeTerminal(terminalId: string, cols: number, rows: number): Promise<void> {
  const res = await fetch(`${API_BASE}/terminals/${encodeURIComponent(terminalId)}/resize`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ cols, rows }),
  });
  if (!res.ok) throw new Error('Failed to resize terminal');
}

export async function closeTerminal(terminalId: string): Promise<void> {
  const res = await fetch(`${API_BASE}/terminals/${encodeURIComponent(terminalId)}`, {
    method: 'DELETE',
  });
  if (!res.ok) throw new Error('Failed to close terminal');
}

export async function listTerminals(): Promise<TerminalSession[]> {
  const res = await fetch(`${API_BASE}/terminals`);
  if (!res.ok) throw new Error('Failed to list terminals');
  return res.json();
}

export function streamTerminalOutput(
  terminalId: string,
  onData: (data: string) => void,
  onExit: (code: number | null) => void,
  onError: (err: string) => void,
  signal?: AbortSignal
): () => void {
  let closed = false;

  fetch(`${API_BASE}/terminals/${encodeURIComponent(terminalId)}/stream`, { signal })
    .then(async (res) => {
      if (!res.ok || !res.body) {
        onError('Failed to open terminal stream');
        return;
      }
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
          if (line.startsWith('event:exit')) continue;
          if (line.startsWith('data:')) {
            const data = line.startsWith('data: ') ? line.slice(6) : line.slice(5);
            if (data === '[DONE]' || data === '[CLOSED]') {
              closed = true;
              onExit(null);
              return;
            }
            try {
              const parsed = JSON.parse(data);
              if (parsed.type === 'data' && parsed.data) {
                onData(parsed.data);
              } else if (parsed.type === 'exit') {
                onExit(parsed.code ?? null);
                closed = true;
                return;
              }
            } catch {
              onData(line + '\n');
            }
          }
        }
      }
      if (!closed) onExit(null);
    })
    .catch((err) => {
      if (err.name !== 'AbortError') onError(err.message || 'Terminal stream error');
    });

  return () => {
    closed = true;
  };
}
