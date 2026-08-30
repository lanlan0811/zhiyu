//! Writing panel (daily mode): AI writing task presets.

import { useState } from 'react';
import type { DaemonApi, SessionRow } from '../lib/api';

const KINDS: Array<[string, string]> = [
  ['longform', '长文写作'],
  ['rewrite', '改写'],
  ['polish', '润色'],
  ['summarize', '摘要'],
  ['translate', '翻译'],
  ['outline', '大纲'],
];

export function WritingPanel({ api, session }: { api: DaemonApi | null; session: SessionRow | null }) {
  const [kind, setKind] = useState('longform');
  const [topic, setTopic] = useState('');
  const [length, setLength] = useState('');
  const [language, setLanguage] = useState('zh-CN');
  const [status, setStatus] = useState('');

  const run = async () => {
    if (!api || !session) {
      setStatus('请先新建一个会话');
      return;
    }
    if (!topic.trim()) {
      setStatus('请填写主题');
      return;
    }
    setStatus('已交给 AI 写作…（在会话面板查看结果）');
    await api.writingRun(session.id, {
      kind,
      topic: topic.trim(),
      length: length.trim() || undefined,
      language,
    });
  };

  return (
    <div style={styles.wrap}>
      <h2 style={styles.title}>AI 写作</h2>
      <div style={styles.kindRow}>
        {KINDS.map(([key, label]) => (
          <button key={key} style={kindBtn(kind === key)} onClick={() => setKind(key)}>
            {label}
          </button>
        ))}
      </div>
      <textarea
        style={styles.topic}
        placeholder="写作主题 / 待改写原文 / 待摘要内容…"
        value={topic}
        onChange={(e) => setTopic(e.target.value)}
        rows={6}
      />
      <div style={styles.row}>
        <input style={styles.input} placeholder="篇幅（如 800 字）" value={length} onChange={(e) => setLength(e.target.value)} />
        <input style={styles.input} placeholder="语言（zh-CN / en）" value={language} onChange={(e) => setLanguage(e.target.value)} />
        <button style={styles.btn} onClick={run}>
          开始写作
        </button>
      </div>
      {status && <div style={styles.status}>{status}</div>}
      <div style={styles.hint}>完成后可在会话中查看，写作结果可导出为 Markdown（保存到知识库或本地）</div>
    </div>
  );
}

function kindBtn(active: boolean): React.CSSProperties {
  return {
    padding: '6px 14px',
    borderRadius: 6,
    border: '1px solid #3a4a5a',
    background: active ? '#0e6ba8' : 'transparent',
    color: active ? '#fff' : '#c8d6e5',
    cursor: 'pointer',
  };
}

const styles: Record<string, React.CSSProperties> = {
  wrap: { padding: 18, overflowY: 'auto', flex: 1 },
  title: { fontSize: 18, marginBottom: 14 },
  kindRow: { display: 'flex', gap: 8, flexWrap: 'wrap', marginBottom: 14 },
  topic: { width: '100%', background: '#15212b', border: '1px solid #24323e', borderRadius: 8, padding: 12, color: '#e8f0f8', resize: 'vertical', outline: 'none', fontSize: 14 },
  row: { display: 'flex', gap: 8, marginTop: 10 },
  input: { flex: 1, background: '#15212b', border: '1px solid #24323e', borderRadius: 6, padding: '8px 10px', color: '#e8f0f8', outline: 'none' },
  btn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 18px', cursor: 'pointer' },
  status: { marginTop: 10, fontSize: 13, color: '#7fd4ff' },
  hint: { marginTop: 16, fontSize: 12, color: '#5a6b7a' },
};
