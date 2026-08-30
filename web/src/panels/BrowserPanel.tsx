//! Browser panel: user + agent dual control of the embedded WebView2 browser.

import { useState } from 'react';
import type { DaemonApi, SessionRow } from '../lib/api';

interface TabInfo {
  id: string;
  url: string;
  title: string;
  origin: string;
}

export function BrowserPanel({ api, session }: { api: DaemonApi | null; session: SessionRow | null }) {
  const [url, setUrl] = useState('https://example.com');
  const [result, setResult] = useState('');
  const [tabs, setTabs] = useState<TabInfo[]>([]);

  const exec = async (method: string, extra: Record<string, unknown> = {}) => {
    if (!api || !session) {
      setResult('请先新建会话');
      return;
    }
    const res = (await api.browserExecute(session.id, { method, ...extra })) as {
      ok: boolean;
      value: unknown;
      elapsedMs: number;
    };
    setResult(JSON.stringify(res.value, null, 2));
    if (method === 'listTabs' || method === 'listUserTabs') {
      setTabs((res.value as TabInfo[]) ?? []);
    }
  };

  return (
    <div style={styles.wrap}>
      <div style={styles.navRow}>
        <input style={styles.input} value={url} onChange={(e) => setUrl(e.target.value)} />
        <button style={styles.btn} onClick={() => exec('navigate', { url })}>
          前往
        </button>
        <button style={styles.btnGhost} onClick={() => exec('back')}>
          ←
        </button>
        <button style={styles.btnGhost} onClick={() => exec('forward')}>
          →
        </button>
        <button style={styles.btnGhost} onClick={() => exec('reload')}>
          刷新
        </button>
        <button style={styles.btnGhost} onClick={() => exec('snapshot')}>
          快照
        </button>
        <button style={styles.btnGhost} onClick={() => exec('listTabs')}>
          Tabs
        </button>
      </div>

      {tabs.length > 0 && (
        <div style={styles.tabs}>
          {tabs.map((t) => (
            <span key={t.id} style={styles.tab}>
              {t.title || t.url} ({t.origin})
            </span>
          ))}
        </div>
      )}

      <div style={styles.agentRow}>
        <input style={styles.input} placeholder="ref（来自快照）" id="browser-ref" />
        <button
          style={styles.btn}
          onClick={() => {
            const ref = (document.getElementById('browser-ref') as HTMLInputElement).value;
            exec('click', { ref });
          }}
        >
          点击
        </button>
        <input style={styles.input} placeholder="输入文本" id="browser-fill" />
        <button
          style={styles.btn}
          onClick={() => {
            const ref = (document.getElementById('browser-ref') as HTMLInputElement).value;
            const value = (document.getElementById('browser-fill') as HTMLInputElement).value;
            exec('fill', { ref, value });
          }}
        >
          输入
        </button>
        <button style={styles.btnGhost} onClick={() => exec('screenshot', { clip: true, fullPage: false })}>
          截图
        </button>
      </div>

      <pre style={styles.result}>{result || '— 浏览器操作结果 —'}</pre>
      <div style={styles.hint}>
        支持：navigate / snapshot / click / fill / press / scroll / evaluate / screenshot / waitFor / 对话框处理
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  wrap: { padding: 14, display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0, gap: 10 },
  navRow: { display: 'flex', gap: 8 },
  agentRow: { display: 'flex', gap: 8 },
  input: { flex: 1, background: '#15212b', border: '1px solid #24323e', borderRadius: 6, padding: '8px 10px', color: '#e8f0f8', outline: 'none' },
  btn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 14px', cursor: 'pointer' },
  btnGhost: { background: 'transparent', border: '1px solid #3a4a5a', color: '#c8d6e5', borderRadius: 6, padding: '8px 12px', cursor: 'pointer' },
  tabs: { display: 'flex', gap: 8, flexWrap: 'wrap' },
  tab: { background: '#15212b', border: '1px solid #24323e', borderRadius: 6, padding: '4px 10px', fontSize: 12, color: '#c8d6e5' },
  result: { flex: 1, background: '#0d141a', border: '1px solid #24323e', borderRadius: 8, padding: 12, overflow: 'auto', fontSize: 12, color: '#9fe08a', whiteSpace: 'pre-wrap', margin: 0 },
  hint: { fontSize: 11, color: '#5a6b7a' },
};
