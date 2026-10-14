import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import AppTitleBar, { WindowFrame } from './AppTitleBar';

const native = vi.hoisted(() => ({
  minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(),
  isMaximized: vi.fn(), onResized: vi.fn(),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => native }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal('__TAURI_INTERNALS__', {});
  vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Windows NT 10.0');
  native.minimize.mockResolvedValue(undefined);
  native.toggleMaximize.mockResolvedValue(undefined);
  native.close.mockResolvedValue(undefined);
  native.isMaximized.mockResolvedValue(false);
  native.onResized.mockResolvedValue(vi.fn());
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe('AppTitleBar', () => {
  it('keeps toolbar actions outside the drag region and invokes native controls', async () => {
    const action = vi.fn();
    const { container } = render(<AppTitleBar><button onClick={action}>Menu</button></AppTitleBar>);
    fireEvent.click(screen.getByRole('button', { name: 'Menu' }));
    expect(action).toHaveBeenCalledOnce();
    expect(screen.getByText('Menu').closest('[data-tauri-drag-region]')).toBeNull();
    expect(container.querySelector('[data-tauri-drag-region]')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '最小化' }));
    fireEvent.click(screen.getByRole('button', { name: '最大化' }));
    fireEvent.click(screen.getByRole('button', { name: '关闭窗口' }));
    expect(native.minimize).toHaveBeenCalledOnce();
    expect(native.toggleMaximize).toHaveBeenCalledOnce();
    expect(native.close).toHaveBeenCalledOnce();
    await waitFor(() => expect(native.onResized).toHaveBeenCalledOnce());
  });

  it('updates restore state after native resizing and releases the listener', async () => {
    const stop = vi.fn();
    native.onResized.mockResolvedValue(stop);
    const { unmount } = render(<AppTitleBar />);
    await waitFor(() => expect(native.onResized).toHaveBeenCalledOnce());
    native.isMaximized.mockResolvedValue(true);
    await act(async () => { await native.onResized.mock.calls[0][0](); });
    expect(screen.getByRole('button', { name: '还原' })).toBeInTheDocument();
    unmount();
    expect(stop).toHaveBeenCalledOnce();
  });

  it('releases a listener that resolves after unmount', async () => {
    let resolve!: (stop: () => void) => void;
    native.onResized.mockReturnValue(new Promise(done => { resolve = done; }));
    const { unmount } = render(<AppTitleBar />);
    unmount();
    const stop = vi.fn();
    await act(async () => { resolve(stop); });
    expect(stop).toHaveBeenCalledOnce();
  });

  it.each(['browser', 'macOS'])('does not add window controls in %s', (platform) => {
    if (platform === 'browser') vi.stubGlobal('__TAURI_INTERNALS__', undefined);
    else vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Macintosh');
    const { container } = render(<AppTitleBar><button>Menu</button></AppTitleBar>);
    expect(screen.getByText('Menu')).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: '窗口控制' })).not.toBeInTheDocument();
    expect(container.querySelector('[data-tauri-drag-region]')).toBeNull();
    expect(native.onResized).not.toHaveBeenCalled();
  });

  it('provides controls on screens without a toolbar', async () => {
    render(<WindowFrame><div>Login</div></WindowFrame>);
    expect(screen.getByText('Login')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '关闭窗口' })).toBeInTheDocument();
    await waitFor(() => expect(native.onResized).toHaveBeenCalledOnce());
  });
});
