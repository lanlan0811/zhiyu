//! Workspace panel (coding mode): file tree, editor, terminal, git.

import { useCallback, useEffect, useState } from 'react';
import type { DaemonApi, SessionRow } from '../lib/api';

interface DirEntry {
  name: string;
  path: string;
  isDir: boolean;
}

export function WorkspacePanel({ api, session }: { api: DaemonApi | null; session: SessionRow | null }) {
  const [entries, setEntries] = useState<DirEntry[]>([]);
  const [currentDir, setCurrentDir] = useState<string | null>(null);
  const [openFile, setOpenFile] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState('');
  const [termInput, setTermInput] = useState('');
  const [termOutput, setTermOutput] = useState('');
  const [workspaceRoot, setWorkspaceRoot] = useState('');

  const refreshDir = useCallback(
    async (rel?: string) => {
      if (!api || !session) return;
      try {
        const list = (await api.workspaceListDir(session.id, rel)) as DirEntry[];
        setEntries(list);
        setCurrentDir(rel ?? null);
      } catch {
        setEntries([]);
      }
    },
    [api, session],
  );

  useEffect(() => {
    // In the real shell the session carries the workspace dir; the UI asks
    // the user to type it in until binding is complete.
    setWorkspaceRoot((session as { workspaceDir?: string } | null)?.workspaceDir ?? '');
    refreshDir();
  }, [session, refreshDir]);

  const open = async (entry: DirEntry) => {
    if (!api || !session) return;
    if (entry.isDir) {
      await refreshDir(entry.path);
    } else {
      const { content } = (await api.workspaceReadFile(session.id, entry.path)) as { content: string };
      setOpenFile(entry.path);
      setFileContent(content);
    }
  };

  const save = async () => {
    if (!api || !session || !openFile) return;
    await api.workspaceWriteFile(session.id, openFile, fileContent);
  };

  const exec = async () => {
    if (!api || !session || !termInput.trim()) return;
    const { output } = (await api.terminalExec(session.id, termInput.trim())) as { output: string };
    setTermOutput((o) => `$ ${termInput.trim()}\n${output}\n\n${o}`);
    setTermInput('');
  };

  const checkpoint = async () => {
    if (!api || !session) return;
    await api.gitCheckpoint(session.id, `manual checkpoint ${new Date().toLocaleTimeString()}`);
  };

  return (
    <div style={styles.wrap}>
      <div style={styles.topRow}>
        <input
          style={styles.input}
          placeholder="工作区目录（如 D:\\projects\\myapp）"
          value={workspaceRoot}
          onChange={(e) => setWorkspaceRoot(e.target.value)}
        />
        <button
          style={styles.btn}
          onClick={async () => {
            if (!api || !session || !workspaceRoot) return;
            await api.workspaceListDir(session.id, undefined);
            await refreshDir();
          }}
        >
          打开工作区
        </button>
        <button style={styles.btnGhost} onClick={checkpoint}>
          Git Checkpoint
        </button>
      </div>

      <div style={styles.body}>
        <div style={styles.tree}>
          <div style={styles.treeHeader}>
            {currentDir ? `📁 ${currentDir}` : '📁 根目录'}
          </div>
          {entries.map((e) => (
            <button key={e.path} style={styles.treeItem} onClick={() => open(e)}>
              {e.isDir ? '📁 ' : '📄 '}
              {e.name}
            </button>
          ))}
          {entries.length === 0 && <div style={styles.empty}>打开工作区后显示文件树</div>}
        </div>

        <div style={styles.editor}>
          {openFile ? (
            <>
              <div style={styles.editorHeader}>
                {openFile}
                <button style={styles.saveBtn} onClick={save}>
                  保存
                </button>
              </div>
              <textarea
                style={styles.code}
                value={fileContent}
                onChange={(e) => setFileContent(e.target.value)}
                spellCheck={false}
              />
            </>
          ) : (
            <div style={styles.empty}>选择文件查看 / 编辑</div>
          )}
        </div>
      </div>

      <div style={styles.terminal}>
        <div style={styles.termHeader}>终端 · 日志分析</div>
        <pre style={styles.termOutput}>{termOutput}</pre>
        <div style={styles.termRow}>
          <span style={styles.termPrompt}>$</span>
          <input
            style={styles.termInput}
            value={termInput}
            onChange={(e) => setTermInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && exec()}
            placeholder="运行命令…"
          />
        </div>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  wrap: { padding: 14, display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0, gap: 10 },
  topRow: { display: 'flex', gap: 8 },
  input: { flex: 1, background: '#15212b', border: '1px solid #24323e', borderRadius: 6, padding: '8px 10px', color: '#e8f0f8', outline: 'none' },
  btn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 16px', cursor: 'pointer' },
  btnGhost: { background: 'transparent', border: '1px solid #3a4a5a', color: '#c8d6e5', borderRadius: 6, padding: '8px 14px', cursor: 'pointer' },
  body: { display: 'flex', flex: 1, minHeight: 0, gap: 10 },
  tree: { width: 220, border: '1px solid #24323e', borderRadius: 8, overflowY: 'auto', padding: 8, background: '#15212b' },
  treeHeader: { fontSize: 12, color: '#7fd4ff', padding: '4px 6px', marginBottom: 4 },
  treeItem: { display: 'block', width: '100%', textAlign: 'left', background: 'transparent', border: 'none', color: '#d5e2ee', padding: '5px 8px', borderRadius: 4, cursor: 'pointer', fontSize: 13, fontFamily: 'monospace' },
  editor: { flex: 1, border: '1px solid #24323e', borderRadius: 8, display: 'flex', flexDirection: 'column', background: '#10181f' },
  editorHeader: { padding: '8px 12px', borderBottom: '1px solid #24323e', color: '#c8d6e5', fontSize: 13, display: 'flex', justifyContent: 'space-between' },
  saveBtn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 4, padding: '2px 12px', cursor: 'pointer' },
  code: { flex: 1, background: '#10181f', border: 'none', color: '#d5e2ee', padding: 12, fontFamily: 'monospace', fontSize: 13, resize: 'none', outline: 'none' },
  terminal: { height: 160, border: '1px solid #24323e', borderRadius: 8, background: '#0d141a', display: 'flex', flexDirection: 'column' },
  termHeader: { padding: '6px 12px', borderBottom: '1px solid #24323e', fontSize: 12, color: '#7fd4ff' },
  termOutput: { flex: 1, overflowY: 'auto', padding: 8, margin: 0, fontSize: 12, color: '#9fe08a', whiteSpace: 'pre-wrap' },
  termRow: { display: 'flex', alignItems: 'center', gap: 6, padding: '0 10px 8px' },
  termPrompt: { color: '#7fd4ff', fontSize: 13 },
  termInput: { flex: 1, background: 'transparent', border: 'none', color: '#e8f0f8', outline: 'none', fontFamily: 'monospace', fontSize: 13 },
  empty: { color: '#5a6b7a', fontSize: 13, textAlign: 'center', marginTop: 40 },
};
