import {fireEvent,render,screen,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {invoke} from '@tauri-apps/api/core';
import InstalledPlugins from './InstalledPlugins';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
vi.mock('../api/projects',()=>({listProjects:vi.fn().mockResolvedValue([{id:'p',name:'Project',workspace_path:'D:/project',is_archived:0,folders:[{path:'D:/project',trusted:true}]}])}));
it('separates legacy preview and import and presents conversion errors',async()=>{
 vi.mocked(invoke).mockImplementation(async(_cmd,args)=>{const r=(args as {request:{action:string;preview?:boolean}}).request;if(r.action==='plugins')return [];if(r.preview)return {notes:['Each command maps to main']};throw new Error('Legacy manifest hooks have no executable handler mapping');});
 render(<InstalledPlugins/>);
 fireEvent.change(screen.getByLabelText('插件包来源'),{target:{value:'D:/legacy-plugin'}});
 fireEvent.click(screen.getByText('预览旧插件'));
 await screen.findByText(/Each command maps to main/);
 expect(invoke).toHaveBeenCalledWith('peripheral_command',{request:{action:'plugin_import_legacy',source:'D:/legacy-plugin',preview:true}});
 fireEvent.click(screen.getByText('确认导入旧插件'));
 await screen.findByRole('alert');expect(screen.getByRole('alert')).toHaveTextContent('no executable handler mapping');
});
it('requires a project and one-use execution approval, exposes revoke while running',async()=>{
 let finish:(v:unknown)=>void=()=>{};
 vi.mocked(invoke).mockImplementation(async(_cmd,args)=>{const request=(args as {request:{action:string}}).request;if(request.action==='plugins')return [{id:'fixture',version:'1.0.0',description:'fixture',enabled:true,commands:['echo']}];if(request.action==='plugin_run')return new Promise(resolve=>{finish=resolve;});return {};});
 render(<InstalledPlugins/>);const run=await screen.findByText('执行 echo');expect(run).toBeDisabled();
 await screen.findByText('Project');fireEvent.change(screen.getByLabelText('插件运行项目'),{target:{value:'p'}});expect(run).toBeDisabled();
 fireEvent.click(screen.getByRole('checkbox'));fireEvent.click(run);
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('peripheral_command',{request:{action:'plugin_run',id:'fixture',command:'echo',input:{},project_id:'p',confirmed:true}}));
 expect(screen.getByRole('checkbox')).not.toBeChecked();expect(screen.getByText('禁用并停止')).toBeEnabled();fireEvent.click(screen.getByText('禁用并停止'));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('peripheral_command',{request:{action:'plugin_enable',id:'fixture',enabled:false}}));finish({stdout:'ok',stderr:''});await screen.findByRole('status');expect(run).toBeDisabled();
});
