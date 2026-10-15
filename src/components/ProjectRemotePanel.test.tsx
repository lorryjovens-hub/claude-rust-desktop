import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import ProjectRemotePanel from './ProjectRemotePanel';

beforeEach(()=>vi.resetAllMocks());
it('requires a current preview and explicit custom command consent',async()=>{
  vi.mocked(invoke).mockResolvedValue({destination:'D:/new-project',files:[{path:'README.md',content:'New project'}],commands:['node setup.cjs']});
  render(<ProjectRemotePanel />);
  const create=screen.getByText('确认创建项目');expect(create).toBeDisabled();
  fireEvent.change(screen.getByLabelText('项目父目录'),{target:{value:'D:/'}});
  fireEvent.change(screen.getByLabelText('新项目名称'),{target:{value:'new-project'}});
  fireEvent.click(screen.getByText('预览项目文件'));await screen.findByText('node setup.cjs');expect(create).toBeDisabled();
  fireEvent.click(screen.getByLabelText('允许执行以上模板命令'));expect(create).toBeEnabled();
  fireEvent.change(screen.getByLabelText('新项目名称'),{target:{value:'other'}});expect(create).toBeDisabled();
});
it('resets one-use remote consent and sends cancellation for the active job',async()=>{
  let finish:(value:unknown)=>void=()=>{};
  vi.mocked(invoke).mockImplementation((_cmd,args)=> (args as {request:{action:string}}).request.action==='network_cancel'?Promise.resolve({}):new Promise(resolve=>{finish=resolve;}));
  render(<ProjectRemotePanel />);
  const approval=screen.getByRole('checkbox',{name:/允许本次修改/});fireEvent.click(approval);
  fireEvent.click(screen.getByText('执行远程操作'));
  await waitFor(()=>expect(screen.getByText('取消远程操作')).toBeEnabled());fireEvent.click(screen.getByText('取消远程操作'));
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('peripheral_command',{request:{action:'network_cancel',id:expect.any(String)}}));
  finish({status_code:200});await waitFor(()=>expect(approval).not.toBeChecked());
});
