import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import ServicesSettings from './ServicesSettings';
const snapshot = {voice:null,team:{endpoint:'http://127.0.0.1:5000',team:'alpha',credential:'TEAM_SYNC_TOKEN'},services:[],memories:[],conflicts:[]};
const action = (args: unknown) => (args as {request?: {action: string}})?.request?.action;
beforeEach(() => {vi.resetAllMocks();vi.mocked(invoke).mockImplementation(async (cmd,args) => cmd === 'peripheral_command' && action(args) === 'status' ? snapshot : {});});
describe('shared service controls', () => {
  it('creates an explicit team memory and starts the real host command', async () => {
    render(<ServicesSettings />);await screen.findByText('保存记忆');
    fireEvent.change(screen.getByLabelText('记忆标题'), {target:{value:'Decision'}});
    fireEvent.change(screen.getByLabelText('记忆内容'), {target:{value:'Use Rust'}});
    fireEvent.click(screen.getByText('保存记忆'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('peripheral_command', {request:{action:'team_put',id:null,title:'Decision',content:'Use Rust',tags:[]}}));
    await screen.findByText('已保存本地记忆，等待同步');fireEvent.click(screen.getByText('启动'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('peripheral_command', {request:{action:'start',interval:60}}));
  });
  it('reports failed authentication without showing success', async () => {
    vi.mocked(invoke).mockImplementation(async (_cmd,args) => {
      if (action(args) === 'team_auth') throw new Error('HTTP 401');return snapshot;
    });
    render(<ServicesSettings />);await screen.findByText('保存记忆');
    fireEvent.click(screen.getByText('认证并保存'));
    expect(await screen.findByRole('alert')).toHaveTextContent('HTTP 401');expect(screen.queryByText('团队认证通过')).not.toBeInTheDocument();
  });
  it('keeps the active credential intact when a new token fails authentication', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd,args) => {
      if (cmd === 'peripheral_command' && action(args) === 'team_auth') throw new Error('HTTP 401');
      return cmd === 'peripheral_command' && action(args) === 'status' ? snapshot : undefined;
    });
    render(<ServicesSettings />);await screen.findByText('保存记忆');
    fireEvent.change(screen.getByLabelText('团队凭据（留空保留）'), {target:{value:'bad-new-token'}});
    fireEvent.click(screen.getByText('认证并保存'));
    await screen.findByRole('alert');
    const args = vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === 'peripheral_credential')![1] as {name:string;value:string};
    expect(args.name).toMatch(/^STAGED_/);expect(args.name).not.toBe('TEAM_SYNC_TOKEN');
    expect(invoke).toHaveBeenCalledWith('peripheral_credential_delete', {name:args.name});
  });
});
