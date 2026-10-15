import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listProjects } from '../api/projects';
import ScheduledPromptsPanel from './ScheduledPromptsPanel';

vi.mock('../api/projects', () => ({listProjects:vi.fn()}));
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(listProjects).mockResolvedValue([
    {id:'trusted',name:'Trusted',workspace_path:'D:/trusted',folders:[{trusted:true}],is_archived:0},
    {id:'untrusted',name:'Untrusted',workspace_path:'D:/untrusted',folders:[{trusted:false}],is_archived:0},
  ] as any);
  vi.mocked(invoke).mockImplementation(async (_name,args) => {
    const request=(args as {request:{action:string;operation?:{operation:string}}}).request;
    if(request.action==='status') return {services:[]} as any;
    if(request.operation?.operation==='list') return {tasks:[]} as any;
    return {};
  });
});
it('creating a schedule does not start a model host and only trusted projects are selectable',async()=>{
  render(<ScheduledPromptsPanel />);
  await screen.findByRole('option',{name:'Trusted'});
  expect(screen.queryByRole('option',{name:'Untrusted'})).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('定时任务项目'),{target:{value:'trusted'}});
  fireEvent.change(screen.getByLabelText('定时提示词'),{target:{value:'Summarize the release'}});
  fireEvent.click(screen.getByText('创建定时任务'));
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('peripheral_command',{request:{action:'cron',project_id:'trusted',operation:{operation:'create',schedule:'0 9 * * *',prompt:'Summarize the release'}}}));
  expect(vi.mocked(invoke).mock.calls.some(([,args])=>(args as any).request.action==='cron_start')).toBe(false);
  expect(screen.getByText('启动定时宿主')).toBeDisabled();
});
