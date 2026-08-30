//! App shell: daemon connection + mode switch + session management.

import { useCallback, useEffect, useRef, useState } from 'react';
import { DaemonApi } from './lib/api';
import { DaemonClient, daemonUrl, type Event } from './lib/daemon';
import type { Mode, SessionRow } from './lib/api';
import { ChatPanel } from './panels/ChatPanel';
import { KnowledgePanel } from './panels/KnowledgePanel';
import { WritingPanel } from './panels/WritingPanel';
import { WorkspacePanel } from './panels/WorkspacePanel';
import { BrowserPanel } from './panels/BrowserPanel';
import { ModelSettingsPanel } from './panels/ModelSettingsPanel';

const DAEMON_PORT = 17691;

export function App() {
  const [mode, setMode] = useState<Mode>('daily');
  const [connected, setConnected] = useState(false);
  const [session, setSession] = useState<SessionRow | null>(null);
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const apiRef = useRef<DaemonApi | null>(null);
  const [transcript, setTranscript] = useState<Array<{ role: string; content: string; reasoning?: string; toolName?: string }>>([]);
  const [streamText, setStreamText] = useState('');
  const [activePanel, setActivePanel] = useState<'chat' | 'knowledge' | 'writing' | 'browser' | 'workspace' | 'models'>('chat');

  // connect to the daemon once
  useEffect(() => {
    let client: DaemonClient | null = null;
    let unsubscribe: (() => void) | null = null;
    (async () => {
      const token = (window as unknown as { __zhiyuToken?: string }).__zhiyuToken ?? 'dev-token';
      client = new DaemonClient(daemonUrl(DAEMON_PORT), token);
      try {
        await client.connect();
        apiRef.current = new DaemonApi(client);
        setConnected(true);
        // live events → transcript
        unsubscribe = client.onEvent((ev) => {
          const sessionId = eventSessionId(ev);
          if (sessionId && sessionRef.current && sessionId !== sessionRef.current.id) return;
          if (ev.type === 'text_delta') {
            setStreamText((t) => t + ev.delta);
          } else if (ev.type === 'turn_finished') {
            setTranscript((t) => [...t, { role: 'assistant', content: streamTextRef.current }]);
            setStreamText('');
          }
        });
        const list = (await apiRef.current.sessionList('daily')) as SessionRow[];
        setSessions(list);
        if (list.length > 0) await openSession(list[0]);
      } catch (e) {
        console.error('daemon connect failed', e);
      }
    })();
    return () => {
      unsubscribe?.();
      client?.close();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const openSession = useCallback(async (row: SessionRow) => {
    setSession(row);
    setTranscript([]);
    setStreamText('');
    const resumed = (await apiRef.current?.sessionResume(row.id, 0)) as Array<{ role: string; content: string; reasoning?: string; toolName?: string }> | undefined;
    if (resumed) setTranscript(resumed.filter((m) => m.role !== 'system' || !m.content.startsWith('【')));
  }, []);

  const refreshSessions = useCallback(async () => {
    if (!apiRef.current) return;
    const list = (await apiRef.current.sessionList(mode)) as SessionRow[];
    setSessions(list);
    // keep the current session highlighted if it belongs to this mode
    if (session && !list.some((s) => s.id === session.id)) setSession(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, session]);

  useEffect(() => {
    refreshSessions();
  }, [mode, refreshSessions]);

  const createSession = useCallback(async () => {
    if (!apiRef.current) return;
    const row = (await apiRef.current.sessionCreate(mode, mode === 'coding' ? '新项目' : '新会话')) as SessionRow;
    await refreshSessions();
    await openSession(row);
  }, [mode, refreshSessions, openSession]);

  const streamTextRef = useRef('');
  useEffect(() => {
    streamTextRef.current = streamText;
  }, [streamText]);

  const sessionRef = useRef<SessionRow | null>(null);
  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  // live events → transcript (wired inside the connect effect above)

  const send = useCallback(
    async (text: string) => {
      if (!apiRef.current || !session) return;
      setTranscript((t) => [...t, { role: 'user', content: text }]);
      await apiRef.current.sessionSend(session.id, text);
    },
    [session],
  );

  return (
    <div style={styles.root}>
      <header style={styles.header}>
        <span style={styles.brand}>知屿 Zhīyǔ</span>
        <nav style={styles.modeSwitch}>
          <button style={modeBtn(mode === 'daily')} onClick={() => setMode('daily')}>
            日常
          </button>
          <button style={modeBtn(mode === 'coding')} onClick={() => setMode('coding')}>
            编码
          </button>
        </nav>
        <span style={styles.status}>{connected ? '● 已连接' : '○ 未连接'}</span>
      </header>

      <div style={styles.body}>
        <aside style={styles.sidebar}>
          <button style={styles.newBtn} onClick={createSession}>
            + 新建会话
          </button>
          <div style={styles.sessionList}>
            {sessions.map((s) => (
              <button
                key={s.id}
                style={sessionBtn(session?.id === s.id)}
                onClick={() => openSession(s)}
              >
                <div style={styles.sessionTitle}>{s.title}</div>
                <div style={styles.sessionMeta}>{new Date(s.updatedAt).toLocaleString()}</div>
              </button>
            ))}
          </div>
          <nav style={styles.panelNav}>
            {panelButtons(mode).map(([key, label]) => (
              <button key={key} style={modeBtn(activePanel === key)} onClick={() => setActivePanel(key as never)}>
                {label}
              </button>
            ))}
          </nav>
        </aside>

        <main style={styles.main}>
          {activePanel === 'chat' && (
            <ChatPanel
              transcript={transcript}
              streamText={streamText}
              onSend={send}
              mode={mode}
              session={session}
              api={apiRef.current}
            />
          )}
          {activePanel === 'knowledge' && mode === 'daily' && <KnowledgePanel api={apiRef.current} />}
          {activePanel === 'writing' && mode === 'daily' && <WritingPanel api={apiRef.current} session={session} />}
          {activePanel === 'browser' && <BrowserPanel api={apiRef.current} session={session} />}
          {activePanel === 'workspace' && mode === 'coding' && <WorkspacePanel api={apiRef.current} session={session} />}
          {activePanel === 'models' && <ModelSettingsPanel api={apiRef.current} />}
        </main>
      </div>
    </div>
  );
}

function panelButtons(mode: Mode): Array<[string, string]> {
  const base: Array<[string, string]> = [['chat', '会话']];
  if (mode === 'daily') {
    base.push(['knowledge', '知识库'], ['writing', '写作'], ['browser', '浏览器']);
  } else {
    base.push(['workspace', '工作区'], ['browser', '浏览器']);
  }
  base.push(['models', '模型设置']);
  return base;
}

/** Extracts the sessionId from an event (most events carry it). */
function eventSessionId(ev: Event): string | null {
  return 'sessionId' in ev ? (ev.sessionId as string | null) : null;
}

function modeBtn(active: boolean): React.CSSProperties {
  return {
    padding: '6px 14px',
    borderRadius: 6,
    border: '1px solid #3a4a5a',
    background: active ? '#0e6ba8' : 'transparent',
    color: active ? '#fff' : '#c8d6e5',
    cursor: 'pointer',
  };
}

function sessionBtn(active: boolean): React.CSSProperties {
  return {
    display: 'block',
    width: '100%',
    textAlign: 'left',
    padding: '8px 10px',
    border: 'none',
    borderLeft: active ? '3px solid #0e6ba8' : '3px solid transparent',
    background: active ? 'rgba(14,107,168,0.15)' : 'transparent',
    color: '#e8f0f8',
    cursor: 'pointer',
    borderRadius: 4,
    marginBottom: 2,
  };
}

const styles: Record<string, React.CSSProperties> = {
  root: { display: 'flex', flexDirection: 'column', height: '100vh', background: '#10181f', color: '#e8f0f8', fontFamily: 'system-ui, sans-serif' },
  header: { display: 'flex', alignItems: 'center', gap: 16, padding: '10px 18px', borderBottom: '1px solid #24323e', background: '#15212b' },
  brand: { fontWeight: 700, fontSize: 16, color: '#7fd4ff' },
  modeSwitch: { display: 'flex', gap: 8 },
  status: { marginLeft: 'auto', fontSize: 12, color: '#7fd4ff' },
  body: { display: 'flex', flex: 1, minHeight: 0 },
  sidebar: { width: 240, borderRight: '1px solid #24323e', padding: 12, display: 'flex', flexDirection: 'column', gap: 8 },
  newBtn: { padding: '8px 12px', borderRadius: 6, border: '1px solid #0e6ba8', background: '#0e6ba8', color: '#fff', cursor: 'pointer' },
  sessionList: { flex: 1, overflowY: 'auto' },
  sessionTitle: { fontSize: 14, fontWeight: 600 },
  sessionMeta: { fontSize: 11, color: '#7a8fa0' },
  panelNav: { display: 'flex', flexDirection: 'column', gap: 6, borderTop: '1px solid #24323e', paddingTop: 10 },
  main: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' },
};
