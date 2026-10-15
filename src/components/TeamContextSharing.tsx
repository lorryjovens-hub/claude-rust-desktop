import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {listProjects,type Project} from '../api/projects';
interface Sharing {team:string;endpoint:string;enabled:boolean}
export default function TeamContextSharing() {
  const [projects,setProjects]=useState<Project[]>([]);const [project,setProject]=useState('');const [sharing,setSharing]=useState<Sharing|null>(null);const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  useEffect(()=>{void listProjects().then(p=>setProjects(p.filter(p=>!p.is_archived&&p.workspace_path&&p.folders.length>0&&p.folders.every(f=>f.trusted)))).catch(e=>setError(String(e)));},[]);
  useEffect(()=>{let active=true;setSharing(null);if(project)void invoke<Sharing|null>('peripheral_command',{request:{action:'team_context',project_id:project,enabled:null}}).then(s=>{if(active)setSharing(s);}).catch(e=>{if(active)setError(String(e));});return()=>{active=false;};},[project]);
  return <section className="space-y-2"><h4>项目模型上下文</h4><p>启用后，此项目聊天会把当前团队的共享记忆发送给所选模型服务商；不发送团队凭据。授权绑定当前团队与服务器。</p>
    <select aria-label="团队上下文项目" className="w-full border bg-claude-bg p-2" disabled={busy} value={project} onChange={e=>setProject(e.target.value)}><option value="">选择已信任项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>
    <p>{sharing?.enabled?`已允许团队 ${sharing.team}（${sharing.endpoint}）`:'未启用'}</p>
    <button className="rounded border p-2" disabled={busy||!project} onClick={async()=>{setBusy(true);setError('');try{setSharing(await invoke<Sharing>('peripheral_command',{request:{action:'team_context',project_id:project,enabled:!sharing?.enabled}}));}catch(e){setError(String(e));}finally{setBusy(false);}}}>{sharing?.enabled?'停用团队上下文':'允许发送团队记忆给模型'}</button>
    {error&&<p role="alert">{error}</p>}
  </section>;
}
