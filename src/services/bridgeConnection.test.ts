import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

const response = (status: string, database: boolean, ok = true) => ({
  ok, json: async () => ({ status, database }),
});

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  delete (window as any).__TAURI_INTERNALS__;
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  delete (window as any).__TAURI_INTERNALS__;
});

describe('backend readiness', () => {
  it('shares one check and waits through connection failures and an unhealthy database', async () => {
    const fetchMock = vi.fn()
      .mockRejectedValueOnce(new TypeError('Failed to fetch'))
      .mockResolvedValueOnce(response('healthy', false))
      .mockResolvedValueOnce(response('healthy', true));
    vi.stubGlobal('fetch', fetchMock);
    const { waitForApiReady } = await import('./bridgeConnection');
    const ready = vi.fn();
    const pending = waitForApiReady();
    expect(waitForApiReady()).toBe(pending);
    void pending.then(ready);
    await vi.advanceTimersByTimeAsync(1500);
    expect(ready).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1500);
    await pending;
    expect(ready).toHaveBeenCalledOnce();
    expect(fetchMock.mock.calls.every(([url]) => url === '/api/health')).toBe(true);
  });

  it('never falls through after a timeout and allows a later retry', async () => {
    const fetchMock = vi.fn().mockResolvedValue(response('starting', false, false));
    vi.stubGlobal('fetch', fetchMock);
    const { waitForApiReady } = await import('./bridgeConnection');
    const failed = expect(waitForApiReady()).rejects.toThrow('等待后端启动超时');
    await vi.advanceTimersByTimeAsync(300_000);
    await failed;
    fetchMock.mockResolvedValue(response('healthy', true));
    await expect(waitForApiReady()).resolves.toBeUndefined();
  });

  it('uses the healthy fallback port and obtains the desktop token before becoming ready', async () => {
    (window as any).__TAURI_INTERNALS__ = {};
    vi.mocked(invoke).mockResolvedValueOnce('test-bridge-key');
    vi.stubGlobal('fetch', vi.fn(async (url: string) => {
      if (url === 'http://127.0.0.1:30086/health') return response('healthy', true);
      throw new TypeError('Failed to fetch');
    }));
    const bridge = await import('./bridgeConnection');
    await bridge.waitForApiReady();
    expect(await bridge.detectBridgePort()).toBe(30086);
    expect(bridge.API_BASE).toBe('http://127.0.0.1:30086/api');
    expect(localStorage.getItem('bridge_api_key')).toBe('test-bridge-key');
  });
});
