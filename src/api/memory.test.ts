import { afterEach, describe, expect, it, vi } from 'vitest';
import { memoryApi } from './memory';

afterEach(() => { vi.unstubAllGlobals(); localStorage.clear(); });

describe('memory bridge API', () => {
  it('uses the browser development proxy and forwards the bridge token', async () => {
    localStorage.setItem('bridge_api_key', 'test-bridge-token');
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    await memoryApi.search('preferences', 5);
    expect(fetchMock).toHaveBeenCalledWith('/api/memory/search', expect.objectContaining({
      method: 'POST', body: JSON.stringify({ query: 'preferences', top_k: 5 }),
      headers: expect.objectContaining({ Authorization: 'Bearer test-bridge-token' }),
    }));
  });

  it('propagates backend failures rather than reporting a successful write', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'Memory is disabled' }), { status: 500 })));
    await expect(memoryApi.ingest('Remember my preferences', 0.8)).rejects.toThrow('Memory is disabled');
  });
});
