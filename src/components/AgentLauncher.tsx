import AgentLegacyImport from './AgentLegacyImport';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listProjects, type Project } from '../api/projects';

interface Definition { id:string; description:string; system_prompt:string; tools:string[]|null }
const field = 'w-full rounded border border-claude-border bg-claude-bg p-2 text-sm';
const command = <T,>(request:Record<string,unknown>) => invoke<T>('peripheral_command',{request});
export default function AgentLauncher({ onOpen }: { onOpen:()=>void }) {
  const [definitions,setDefinitions] = useState<Definition[]>([]);
  const [projects,setProjects] = useState<Project[]>([]);
  const [agent,setAgent] = useState('general');
  const [project,setProject] = useState('');
  const [provider,setProvider] = useState('');
  const [model,setModel] = useState('');
  const [custom,setCustom] = useState('');
  const [error,setError] = useState('');
  const [busy,setBusy] = useState(false);
  useEffect(()=>{
    Promise.all([command<Definition[]>({action:'agents'}),listProjects()]).then(([defs,items])=>{setDefinitions(defs);setProjects(items.filter(p=>!p.is_archived&&p.workspace_path&&p.folders.length>0&&p.folders.every(f=>f.trusted)));}).catch(e=>setError(String(e)));
  },[]);
  const run = async (operation:()=>Promise<void>) => {setBusy(true);setError('');try{await operation();}catch(e){setError(String(e));}finally{setBusy(false);}};
  return <section className="mb-4 space-y-3 rounded border border-claude-border p-3">
    <h3>运行共享 Agent</h3>
    <p className="text-xs">选择类型后打开项目会话，再输入任务运行。结果、工具轨迹、权限确认和取消均使用聊天窗口；探索、规划与指南限制为只读工具。</p>
    {error&&<p role="alert">{error}</p>}
    <select aria-label="Agent 类型" className={field} value={agent} onChange={e=>setAgent(e.target.value)}>{definitions.map(d=><option key={d.id} value={d.id}>{d.id}</option>)}</select>
    <p className="text-xs">{definitions.find(d=>d.id===agent)?.description}</p>
    <select aria-label="Agent 项目" className={field} value={project} onChange={e=>setProject(e.target.value)}><option value="">选择已信任项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>
    <input aria-label="Agent Provider ID" className={field} value={provider} onChange={e=>setProvider(e.target.value)} placeholder="已保存的 Provider ID" />
    <input aria-label="Agent 模型" className={field} value={model} onChange={e=>setModel(e.target.value)} placeholder="模型 ID" />
    <button disabled={busy||!project||!provider||!model||definitions.length===0} className="rounded border p-2 text-sm disabled:opacity-40" onClick={()=>void run(async()=>{
      const result=await command<{id:string}>({action:'agent_create',project_id:project,agent_type:agent,provider,model});
      window.dispatchEvent(new CustomEvent('conversationCreated',{detail:{id:result.id}}));
      window.dispatchEvent(new CustomEvent('navigateToConversation',{detail:{id:result.id}}));
      onOpen();
    })}>打开 Agent 会话</button>
    <AgentLegacyImport onImported={async () => setDefinitions(await command<Definition[]>({action:'agents'}))} />
    <details><summary className="text-sm">导入自定义 Agent</summary>
      <p className="text-xs">JSON 字段：id、description、system_prompt、tools。tools 为允许的工具名数组，null 表示沿用宿主工具与权限。</p>
      <textarea aria-label="自定义 Agent JSON" rows={4} className={field} value={custom} onChange={e=>setCustom(e.target.value)} />
      <button disabled={busy||!custom.trim()} className="rounded border p-2 text-sm" onClick={()=>void run(async()=>{await command({action:'agent_import',definition:JSON.parse(custom)});setDefinitions(await command<Definition[]>({action:'agents'}));setCustom('');})}>导入 Agent 定义</button>
    </details>
  </section>;
}
