const DEFAULT_BRIDGE_PORT = 30085;
const STARTUP_TIMEOUT_MS = 300_000;
const RETRY_INTERVAL_MS = 1500;

export const isTauriApp = typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__;
export const useDevProxy = import.meta.env.DEV && !isTauriApp;
export let API_BASE = useDevProxy ? '/api' : `http://127.0.0.1:${DEFAULT_BRIDGE_PORT}/api`;

let detectedBridgePort: number | null = null;
let apiReady: Promise<void> | null = null;

export function resetBridgePort(): void {
  detectedBridgePort = null;
  apiReady = null;
  API_BASE = useDevProxy ? '/api' : `http://127.0.0.1:${DEFAULT_BRIDGE_PORT}/api`;
}

async function isHealthy(url: string, timeout: number): Promise<boolean> {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(timeout), cache: 'no-store' });
    if (!response.ok) return false;
    const health = await response.json();
    return health.status === 'healthy' && health.database === true;
  } catch {
    return false;
  }
}

async function connect(): Promise<void> {
  // The standalone production web app also supports local-only storage.
  if (!isTauriApp && !useDevProxy) return;
  const deadline = Date.now() + STARTUP_TIMEOUT_MS;
  while (Date.now() < deadline) {
    let port: number | undefined;
    const timeout = Math.max(1, Math.min(3000, deadline - Date.now()));
    if (useDevProxy) {
      if (await isHealthy('/api/health', timeout)) port = DEFAULT_BRIDGE_PORT;
    } else {
      // Match the five fallback ports used by BridgeServer::start.
      const ports = Array.from({ length: 5 }, (_, index) => DEFAULT_BRIDGE_PORT + index);
      const results = await Promise.all(ports.map(candidate => isHealthy(`http://127.0.0.1:${candidate}/health`, timeout)));
      port = ports.find((_, index) => results[index]);
    }
    if (port !== undefined) {
      if (isTauriApp) {
        const { invoke } = await import('@tauri-apps/api/core');
        const key = await invoke<string>('get_bridge_api_key');
        if (!key) throw new Error('未能获取后端连接凭据，请重试。');
        localStorage.setItem('bridge_api_key', key);
        API_BASE = `http://127.0.0.1:${port}/api`;
      }
      detectedBridgePort = port;
      return;
    }
    const remaining = deadline - Date.now();
    if (remaining > 0) await new Promise(resolve => setTimeout(resolve, Math.min(RETRY_INTERVAL_MS, remaining)));
  }
  throw new Error('等待后端启动超时，请检查启动终端中的编译或运行错误，然后重试。');
}

export function waitForApiReady(): Promise<void> {
  if (!apiReady) {
    const pending = connect().catch(error => {
      if (apiReady === pending) apiReady = null;
      throw error;
    });
    apiReady = pending;
  }
  return apiReady;
}

export async function detectBridgePort(): Promise<number> {
  await waitForApiReady();
  return detectedBridgePort ?? DEFAULT_BRIDGE_PORT;
}
