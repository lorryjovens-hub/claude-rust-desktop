import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import WorkDetails from '../../src/features/trajectory/WorkDetails';
import { parseBadRequest, readChatResponseError } from '../../src/utils/chatErrors';
import '../../src/index.css';

const detail = JSON.stringify({ detail: "The 'claude-sonnet-4-5' model is not supported when using Codex with a ChatGPT account." });
function Preview() {
  const [error, setError] = useState<ReturnType<typeof parseBadRequest>>(null);
  const [dark, setDark] = useState(false);
  return <main style={{ maxWidth: 760, margin: '64px auto', padding: '0 24px', color: 'var(--text-claude-main)' }}>
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, marginBottom: 32, fontSize: 14 }}>
      <button onClick={async () => setError(parseBadRequest(await readChatResponseError(new Response(detail, { status: 400 }))))}>模拟 HTTP 400</button>
      <button onClick={() => { document.documentElement.classList.toggle('dark'); setDark(!dark); }}>{dark ? '切换浅色' : '切换深色'}</button>
    </div>
    <WorkDetails index={0} live={false} message={{ content: '已读取 package.json，并运行了检查命令。', requestError: error }} />
  </main>;
}
createRoot(document.getElementById('root')!).render(<Preview />);
