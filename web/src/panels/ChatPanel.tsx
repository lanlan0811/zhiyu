//! Chat panel: transcript + composer + thought-level selector + usage ring.

import { useCallback, useEffect, useState } from 'react';
import type { ContextUsage, DaemonApi, Mode, SessionRow } from '../lib/api';

interface Msg {
  role: string;
  content: string;
  reasoning?: string;
  toolName?: string;
}

const LEVELS = ['off', 'low', 'medium', 'high', 'xhigh', 'max'] as const;

export function ChatPanel({
  transcript,
  streamText,
  onSend,
  mode,
  session,
  api,
}: {
  transcript: Msg[];
  streamText: string;
  onSend: (text: string) => void;
  mode: Mode;
  session: SessionRow | null;
  api: DaemonApi | null;
}) {
  const [input, setInput] = useState('');
  const [thought, setThought] = useState<string>(mode === 'coding' ? 'high' : 'medium');
  const [usage, setUsage] = useState<ContextUsage | null>(null);

  const refreshUsage = useCallback(async () => {
    if (!session || !api) return;
    try {
      setUsage((await api.sessionContextUsage(session.id)) as ContextUsage);
    } catch {
      /* usage not ready yet */
    }
  }, [session, api]);

  useEffect(() => {
    refreshUsage();
    const t = setInterval(refreshUsage, 5000);
    return () => clearInterval(t);
  }, [refreshUsage]);

  const submit = () => {
    if (!input.trim()) return;
    onSend(input.trim());
    setInput('');
  };

  const compact = async () => {
    if (!session || !api) return;
    await api.sessionCompact(session.id);
    refreshUsage();
  };

  const percent = usage ? Math.min(100, Math.round((usage.usedTokens / Math.max(usage.maxTokens, 1)) * 100)) : 0;

  return (
    <div style={styles.wrap}>
      <div style={styles.toolbar}>
        <span style={styles.thoughtLabel}>思考强度</span>
        <select
          style={styles.select}
          value={thought}
          onChange={async (e) => {
            const v = e.target.value;
            setThought(v);
            if (session && api) await api.sessionSetThoughtLevel(session.id, v);
          }}
        >
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        {usage && (
          <div style={styles.usage} title={`已用 ${usage.usedTokens} / ${usage.maxTokens} tokens`}>
            <svg width="26" height="26" viewBox="0 0 26 26">
              <circle cx="13" cy="13" r="10" fill="none" stroke="#24323e" strokeWidth="4" />
              <circle
                cx="13"
                cy="13"
                r="10"
                fill="none"
                stroke={percent > 85 ? '#e0533d' : '#7fd4ff'}
                strokeWidth="4"
                strokeDasharray={`${(percent / 100) * 62.8} 62.8`}
                transform="rotate(-90 13 13)"
              />
            </svg>
            <span style={styles.usageText}>{percent}%</span>
          </div>
        )}
        <button style={styles.compactBtn} onClick={compact} disabled={!session}>
          /compact
        </button>
      </div>

      <div style={styles.transcript}>
        {transcript.map((m, i) => (
          <div key={i} style={msgStyle(m.role)}>
            {m.reasoning && <details style={styles.reasoning}><summary>思考</summary>{m.reasoning}</details>}
            {m.toolName && <div style={styles.toolTag}>🔧 {m.toolName}</div>}
            <div style={styles.msgText}>{m.content}</div>
          </div>
        ))}
        {streamText && <div style={msgStyle('assistant')}>{streamText}</div>}
        {transcript.length === 0 && !streamText && (
          <div style={styles.empty}>输入消息开始对话 · 按 Enter 发送</div>
        )}
      </div>

      <div style={styles.composer}>
        <textarea
          style={styles.textarea}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder={mode === 'coding' ? '询问代码、运行命令、审查 diff…' : '提问、写作、知识库问答…'}
          rows={3}
        />
        <div style={styles.composerRow}>
          <span style={styles.hint}>/think 切换思考强度 · /compact 压缩上下文</span>
          <button style={styles.sendBtn} onClick={submit} disabled={!input.trim()}>
            发送
          </button>
        </div>
      </div>
    </div>
  );
}

function msgStyle(role: string): React.CSSProperties {
  return {
    padding: '10px 14px',
    borderRadius: 8,
    background: role === 'user' ? '#1c2c3a' : '#15212b',
    border: '1px solid #24323e',
    margin: '6px 0',
    whiteSpace: 'pre-wrap',
  };
}

const styles: Record<string, React.CSSProperties> = {
  wrap: { display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0, padding: 14 },
  toolbar: { display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 },
  thoughtLabel: { fontSize: 13, color: '#c8d6e5' },
  select: { background: '#15212b', color: '#e8f0f8', border: '1px solid #24323e', borderRadius: 6, padding: '4px 8px' },
  usage: { display: 'flex', alignItems: 'center', gap: 4, marginLeft: 'auto' },
  usageText: { fontSize: 12, color: '#c8d6e5' },
  compactBtn: { background: 'transparent', border: '1px solid #3a4a5a', color: '#c8d6e5', borderRadius: 6, padding: '4px 10px', cursor: 'pointer' },
  transcript: { flex: 1, overflowY: 'auto', marginBottom: 10 },
  reasoning: { fontSize: 13, color: '#9aa7b3', marginBottom: 6 },
  toolTag: { fontSize: 12, color: '#7fd4ff', marginBottom: 4 },
  msgText: { fontSize: 14, lineHeight: 1.6 },
  empty: { textAlign: 'center', color: '#5a6b7a', marginTop: 60, fontSize: 14 },
  composer: { border: '1px solid #24323e', borderRadius: 10, padding: 10, background: '#15212b' },
  textarea: { width: '100%', background: 'transparent', border: 'none', color: '#e8f0f8', fontSize: 14, resize: 'none', outline: 'none' },
  composerRow: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 6 },
  hint: { fontSize: 11, color: '#5a6b7a' },
  sendBtn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 6, padding: '6px 16px', cursor: 'pointer' },
};
