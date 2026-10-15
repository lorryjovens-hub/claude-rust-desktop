import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import MagicDocsPanel from './MagicDocsPanel';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
vi.mock('../api/projects',()=>({listProjects:vi.fn().mockResolvedValue([{id:'p',name:'Project',workspace_path:'D:/project',is_archived:0,folders:[{path:'D:/project',trusted:true}]}])}));
const snapshot = {documents:[{id:'d',path:'notes.md',title:'Architecture',auto_update:false,update_count:0}],previews:[{id:'preview',document_id:'d',original:'Original text',proposed:'Revised text'}],services:[]};
beforeEach(()=>{vi.mocked(invoke).mockReset().mockResolvedValue(snapshot);});
it('shows both document versions and only applies after the explicit action',async()=>{
  render(<MagicDocsPanel />);
  await screen.findByText('Project');
  fireEvent.change(screen.getByLabelText('文档项目'),{target:{value:'p'}});
  expect(await screen.findByText('Original text')).toBeInTheDocument();
  expect(screen.getByText('Revised text')).toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalledWith('peripheral_command',{request:{action:'magic',project_id:'p',operation:{action:'apply',id:'preview'}}});
  fireEvent.click(screen.getByText('确认应用此预览'));
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('peripheral_command',{request:{action:'magic',project_id:'p',operation:{action:'apply',id:'preview'}}}));
});
