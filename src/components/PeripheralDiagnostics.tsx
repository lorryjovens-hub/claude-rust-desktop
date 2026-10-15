import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export default function PeripheralDiagnostics() {
  const [concurrency,setConcurrency]=useState(4); const [iterations,setIterations]=useState(10);
  const [url,setUrl]=useState(''); const [busy,setBusy]=useState(false); const [error,setError]=useState(''); const [result,setResult]=useState('');
  const run=async(request:Record<string,unknown>)=>{setBusy(true);setError('');setResult('');try{setResult(JSON.stringify(await invoke('peripheral_command',{request}),null,2));}catch(e){setError(String(e));}finally{setBusy(false);}};
  return <section className="space-y-3 rounded border border-claude-border p-3">
    <h3>本地诊断与更新检查</h3>
    <p className="text-sm">诊断使用临时本机 HTTP 服务，统计请求和响应头耗时，不调用付费模型；结果不代表模型性能或官方产品对比。</p>
    <label>并发<input aria-label="诊断并发" type="number" min="1" max="64" value={concurrency} onChange={e=>setConcurrency(Number(e.target.value))} className="ml-2 w-20 border bg-claude-bg" /></label>
    <label>每路请求数<input aria-label="诊断请求数" type="number" min="1" max="1000" value={iterations} onChange={e=>setIterations(Number(e.target.value))} className="ml-2 w-20 border bg-claude-bg" /></label>
    <button disabled={busy||!Number.isInteger(concurrency)||!Number.isInteger(iterations)||concurrency<1||concurrency>64||iterations<1||iterations>1000||concurrency*iterations>10000} className="rounded border p-2 text-sm" onClick={()=>void run({action:'diagnostics',concurrency,iterations})}>运行本地诊断</button>
    <input aria-label="更新清单地址" value={url} onChange={e=>setUrl(e.target.value)} className="w-full rounded border bg-claude-bg p-2 text-sm" placeholder="发布方提供的 HTTPS manifest.json 地址" />
    <button disabled={busy||!url} className="rounded border p-2 text-sm" onClick={()=>void run({action:'update_check',url})}>检查此更新源</button>
    <p className="text-xs">仅检查版本信息，不自动下载或安装。</p>
    {error&&<p role="alert">{error}</p>}{result&&<pre role="status" className="whitespace-pre-wrap text-xs">{result}</pre>}
  </section>;
}
