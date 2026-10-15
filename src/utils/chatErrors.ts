export interface RequestErrorDetails {
  status: number;
  detail: string;
}

/** Keep the HTTP status and the unmodified response body across string callbacks. */
export async function readChatResponseError(response: Response): Promise<string> {
  const body = await response.text();
  return `HTTP ${response.status}${body ? `\n${body}` : ''}`;
}

export function formatChatStreamError(event: { error?: unknown; detail?: unknown; status?: number; status_code?: number }): string {
  const raw = event.error;
  const error = typeof raw === 'string' ? raw : raw ? JSON.stringify(raw) : '未知错误';
  const detail = event.detail ? `\n${typeof event.detail === 'string' ? event.detail : JSON.stringify(event.detail)}` : '';
  const status = event.status ?? event.status_code;
  return `${status ? `HTTP ${status}\n` : ''}${error}${detail}`;
}

export function parseBadRequest(error: string): RequestErrorDetails | null {
  const text = error.trim();
  const status = /(?:\bHTTP(?:\/\d(?:\.\d)?)?\s+|\bstatus(?:_code|Code)?[\s"':=]+|\bAPI (?:error|错误)[:\s]+)400\b/i;
  if (status.test(text) || /^(?:Error:\s*)?400\b/.test(text)) {
    return { status: 400, detail: text.replace(/^HTTP 400\s*\n?/, '').trim() || 'Bad Request' };
  }
  return null;
}

export function formatChatError(err: string): string {
  const lower = (err || '').toLowerCase();
  if (lower.includes('quota_exceeded') || lower.includes('额度已用完') || lower.includes('额度已用尽') || lower.includes('时段额度') || lower.includes('周期额度')) {
    return '⚠️ 当前额度已用完，请等待额度重置后再试。你可以在设置页查看额度详情。';
  }
  if (lower.includes('订阅已过期') || lower.includes('未激活') || lower.includes('inactive') || lower.includes('expired')) {
    return '⚠️ 你的订阅已过期或未激活，请续费后继续使用。';
  }
  if (lower.includes('invalid api key') || lower.includes('authentication')) {
    return '⚠️ API 认证失败，请重新登录。';
  }
  if (lower.includes('overloaded') || lower.includes('rate limit') || lower.includes('529')) {
    return '⚠️ 服务暂时繁忙，请稍后再试。';
  }
  return 'Error: ' + err;
}
