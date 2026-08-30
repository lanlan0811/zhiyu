//! Knowledge base panel (daily mode): import, list, search, delete.

import { useCallback, useEffect, useState } from 'react';
import type { DaemonApi, KbDocument, SearchHit } from '../lib/api';

export function KnowledgePanel({ api }: { api: DaemonApi | null }) {
  const [docs, setDocs] = useState<KbDocument[]>([]);
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [importPath, setImportPath] = useState('');
  const [status, setStatus] = useState('');

  const refresh = useCallback(async () => {
    if (!api) return;
    setDocs((await api.knowledgeList()) as KbDocument[]);
  }, [api]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const doImport = async () => {
    if (!api || !importPath.trim()) return;
    setStatus('导入中…');
    try {
      const doc = (await api.knowledgeImport(importPath.trim())) as KbDocument;
      setStatus(`已导入「${doc.title}」（${doc.chunkCount} 个分块）`);
      setImportPath('');
      refresh();
    } catch (e) {
      setStatus(`导入失败：${String(e)}`);
    }
  };

  const doSearch = async () => {
    if (!api || !query.trim()) return;
    setHits((await api.knowledgeSearch(query.trim(), 8)) as SearchHit[]);
  };

  return (
    <div style={styles.wrap}>
      <h2 style={styles.title}>知识库</h2>

      <div style={styles.importRow}>
        <input
          style={styles.input}
          placeholder="文件或目录路径（md/txt/代码/PDF/Word）"
          value={importPath}
          onChange={(e) => setImportPath(e.target.value)}
        />
        <button style={styles.btn} onClick={doImport}>
          导入
        </button>
        <button
          style={styles.btnGhost}
          onClick={async () => {
            if (!api) return;
            await api.knowledgeReindex();
            setStatus('已重建索引');
            refresh();
          }}
        >
          重建索引
        </button>
      </div>
      {status && <div style={styles.status}>{status}</div>}

      <div style={styles.searchRow}>
        <input
          style={styles.input}
          placeholder="混合检索（全文 + 向量）…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && doSearch()}
        />
        <button style={styles.btn} onClick={doSearch}>
          检索
        </button>
      </div>

      {hits.length > 0 && (
        <div style={styles.hits}>
          <h3 style={styles.subTitle}>检索结果（附来源）</h3>
          {hits.map((h, i) => (
            <div key={i} style={styles.hit}>
              <div style={styles.hitMeta}>
                {h.title} · 第 {h.chunkIndex + 1} 块 · 分数 {h.score.toFixed(3)}
                {h.heading && ` · ${h.heading}`}
              </div>
              <div style={styles.hitContent}>{h.content}</div>
            </div>
          ))}
        </div>
      )}

      <h3 style={styles.subTitle}>文档（{docs.length}）</h3>
      <div style={styles.docs}>
        {docs.map((d) => (
          <div key={d.id} style={styles.doc}>
            <div style={styles.docTitle}>{d.title}</div>
            <div style={styles.docMeta}>
              {d.path} · {d.chunkCount} 块
            </div>
            <button
              style={styles.deleteBtn}
              onClick={async () => {
                if (!api) return;
                await api.knowledgeDelete(d.id);
                refresh();
              }}
            >
              删除
            </button>
          </div>
        ))}
        {docs.length === 0 && <div style={styles.empty}>还没有文档，导入一个文件开始</div>}
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  wrap: { padding: 18, overflowY: 'auto', flex: 1 },
  title: { fontSize: 18, marginBottom: 12 },
  importRow: { display: 'flex', gap: 8, marginBottom: 8 },
  searchRow: { display: 'flex', gap: 8, marginBottom: 14 },
  input: { flex: 1, background: '#15212b', border: '1px solid #24323e', borderRadius: 6, padding: '8px 10px', color: '#e8f0f8', outline: 'none' },
  btn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 16px', cursor: 'pointer' },
  btnGhost: { background: 'transparent', border: '1px solid #3a4a5a', color: '#c8d6e5', borderRadius: 6, padding: '8px 14px', cursor: 'pointer' },
  status: { fontSize: 12, color: '#7fd4ff', marginBottom: 8 },
  subTitle: { fontSize: 15, margin: '14px 0 8px' },
  hits: { marginBottom: 12 },
  hit: { background: '#15212b', border: '1px solid #24323e', borderRadius: 8, padding: 10, marginBottom: 8 },
  hitMeta: { fontSize: 12, color: '#7fd4ff', marginBottom: 4 },
  hitContent: { fontSize: 13, lineHeight: 1.6, color: '#d5e2ee' },
  docs: { display: 'flex', flexDirection: 'column', gap: 8 },
  doc: { background: '#15212b', border: '1px solid #24323e', borderRadius: 8, padding: 10 },
  docTitle: { fontSize: 14, fontWeight: 600 },
  docMeta: { fontSize: 12, color: '#7a8fa0', margin: '2px 0 6px' },
  deleteBtn: { background: 'transparent', border: '1px solid #a04030', color: '#e08070', borderRadius: 4, padding: '3px 10px', cursor: 'pointer', fontSize: 12 },
  empty: { color: '#5a6b7a', fontSize: 13 },
};
