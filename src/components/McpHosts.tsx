import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
interface Host {id:string;pid:number;servers:string[]}
export default function McpHosts() {
  const [hosts,setHosts]=useState<Host[]>([]); const [busy,setBusy]=useState(false); const [message,setMessage]=useState('');
  const refresh=async()=>setHosts(await invoke<Host[]>('peripheral_command',{request:{action:'mcp_hosts'}}));
  return <section className="space-y-2"><h3>MCP 运行宿主</h3><p>重启会中断所选宿主中的该服务器。多个 CLI/桌面进程分别列出。</p>
    <button disabled={busy} className="rounded border p-2" onClick={()=>void refresh().catch(e=>setMessage(String(e)))}>刷新运行宿主</button>
    {hosts.filter(h=>h.servers.length>0).map(h=><article key={h.id}><p>进程 {h.pid} · {h.id}</p>{h.servers.map(server=><button key={server} disabled={busy} className="rounded border p-2" onClick={async()=>{setBusy(true);try{await invoke('peripheral_command',{request:{action:'mcp_restart',server,host:h.id}});setMessage(`已重启 ${server}`);await refresh();}catch(e){setMessage(String(e));}finally{setBusy(false);}}}>重启 {server}</button>)}</article>)}
    {message&&<p role="status">{message}</p>}
  </section>;
}
