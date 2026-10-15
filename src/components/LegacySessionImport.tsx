import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {listProjects,type Project} from '../api/projects';
export default function LegacySessionImport() {
 const [projects,setProjects]=useState<Project[]>([]);const [project,setProject]=useState('');const [source,setSource]=useState('');const [busy,setBusy]=useState(false);const [result,setResult]=useState('');const [error,setError]=useState('');
 useEffect(()=>{void listProjects().then(p=>setProjects(p.filter(p=>!p.is_archived&&p.workspace_path&&p.folders.length>0&&p.folders.every(f=>f.trusted)))).catch(e=>setError(String(e)));},[]);
 const run=async(preview:boolean)=>{setBusy(true);setError('');try{setResult(JSON.stringify(await invoke('peripheral_command',{request:{action:'session_import',source,project_id:project,preview}}),null,2));if(!preview)window.dispatchEvent(new CustomEvent('conversationCreated'));}catch(e){setError(String(e));}finally{setBusy(false);}};
 return <section className="space-y-2"><h3>旧 CLI 会话导入</h3><p>选择旧 sessions 目录或单个 JSON 会话，将历史映射到所选项目。保留原 ID、时间及文本；源文件保持不变。</p>
 <select aria-label="会话导入项目" className="w-full border bg-claude-bg p-2" disabled={busy} value={project} onChange={e=>setProject(e.target.value)}><option value="">选择已信任项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select>
 <input aria-label="旧会话来源" className="w-full border bg-claude-bg p-2" value={source} onChange={e=>setSource(e.target.value)}/>
 <button className="rounded border p-2" disabled={busy||!project||!source} onClick={()=>void run(true)}>预览会话导入</button>
 <button className="rounded border p-2" disabled={busy||!project||!source} onClick={()=>void run(false)}>备份并导入会话</button>
 {error&&<p role="alert">{error}</p>}{result&&<pre role="status" className="whitespace-pre-wrap break-all text-xs">{result}</pre>}
 </section>;
}
