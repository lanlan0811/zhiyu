//! Model settings panel: built-in catalogue (window/output/interface/levels),
//! API-key management, custom models.

import { useCallback, useEffect, useState } from 'react';
import type { DaemonApi, ModelConfig } from '../lib/api';

const LEVELS = ['off', 'low', 'medium', 'high', 'xhigh', 'max'];

export function ModelSettingsPanel({ api }: { api: DaemonApi | null }) {
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [keys, setKeys] = useState<Array<{ provider: string; keys: Array<{ id: string; key: string; isDefault: boolean }> }>>([]);
  const [provider, setProvider] = useState('deepseek');
  const [newKey, setNewKey] = useState('');
  const [status, setStatus] = useState('');

  const refresh = useCallback(async () => {
    if (!api) return;
    setModels((await api.modelList()) as ModelConfig[]);
    setKeys((await api.keyList()) as Array<{ provider: string; keys: Array<{ id: string; key: string; isDefault: boolean }> }>);
  }, [api]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const saveKey = async () => {
    if (!api || !newKey.trim()) return;
    await api.keySave(provider, newKey.trim());
    setNewKey('');
    setStatus(`已保存 ${provider} 的 API-Key（加密存储）`);
    refresh();
  };

  return (
    <div style={styles.wrap}>
      <h2 style={styles.title}>模型设置</h2>

      <h3 style={styles.subTitle}>内置模型目录（可覆盖 / 可自定义）</h3>
      <div style={styles.models}>
        {models.map((m) => (
          <div key={m.id} style={styles.model}>
            <div style={styles.modelHeader}>
              <span style={styles.modelName}>{m.name}</span>
              <span style={styles.vendor}>{m.vendor}</span>
            </div>
            <div style={styles.modelMeta}>
              接口：{m.apiFormat} · 窗口：{formatTokens(m.contextWindow)} · 输出：{formatTokens(m.maxOutputTokens)}
            </div>
            <div style={styles.modelMeta}>
              思考档位：{m.reasoning.levels.map((l) => l.value).join(' / ')}（默认 {m.reasoning.defaultLevel}）
            </div>
            <div style={styles.modelMeta}>{m.baseUrl}</div>
          </div>
        ))}
        {models.length === 0 && <div style={styles.empty}>加载模型目录中…</div>}
      </div>

      <h3 style={styles.subTitle}>API-Key（本地加密存储 · 按 provider 独立）</h3>
      <div style={styles.keyRow}>
        <select style={styles.select} value={provider} onChange={(e) => setProvider(e.target.value)}>
          {keys.map((k) => (
            <option key={k.provider} value={k.provider}>
              {k.provider}
            </option>
          ))}
          <option value="deepseek">deepseek</option>
          <option value="glm">glm</option>
        </select>
        <input
          style={styles.input}
          placeholder="sk-…（明文不落盘，DPAPI 加密）"
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          type="password"
        />
        <button style={styles.btn} onClick={saveKey}>
          保存
        </button>
      </div>
      {status && <div style={styles.status}>{status}</div>}
      <div style={styles.keyList}>
        {keys.map((k) => (
          <div key={k.provider} style={styles.keyItem}>
            <span style={styles.keyProvider}>{k.provider}</span>
            {k.keys.map((key) => (
              <span key={key.id} style={styles.keyTag}>
                {key.isDefault ? '默认' : key.id} · {mask(key.key)}
              </span>
            ))}
          </div>
        ))}
      </div>

      <h3 style={styles.subTitle}>思考强度默认档位</h3>
      <div style={styles.levelRow}>
        <span style={styles.label}>日常模式</span>
        <select style={styles.select} defaultValue="medium">
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        <span style={styles.label}>编码模式</span>
        <select style={styles.select} defaultValue="high">
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
      </div>
      <div style={styles.hint}>
        内置模型仅为默认值，可在 ~/.zhiyu/models.json 中覆盖 base_url / contextWindow / 接口 / 档位，也可新建自定义模型
      </div>
    </div>
  );
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(0)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return String(n);
}

function mask(key: string): string {
  if (key.length <= 8) return '••••';
  return `${key.slice(0, 4)}••••${key.slice(-4)}`;
}

const styles: Record<string, React.CSSProperties> = {
  wrap: { padding: 18, overflowY: 'auto', flex: 1 },
  title: { fontSize: 18, marginBottom: 12 },
  subTitle: { fontSize: 15, margin: '16px 0 8px' },
  models: { display: 'flex', flexDirection: 'column', gap: 8 },
  model: { background: '#15212b', border: '1px solid #24323e', borderRadius: 8, padding: 10 },
  modelHeader: { display: 'flex', gap: 8, alignItems: 'baseline' },
  modelName: { fontSize: 14, fontWeight: 600 },
  vendor: { fontSize: 12, color: '#7fd4ff' },
  modelMeta: { fontSize: 12, color: '#9aa7b3', marginTop: 2 },
  keyRow: { display: 'flex', gap: 8 },
  input: { flex: 1, background: '#15212b', border: '1px solid #24323e', borderRadius: 6, padding: '8px 10px', color: '#e8f0f8', outline: 'none' },
  select: { background: '#15212b', color: '#e8f0f8', border: '1px solid #24323e', borderRadius: 6, padding: '6px 8px' },
  btn: { background: '#0e6ba8', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 16px', cursor: 'pointer' },
  status: { marginTop: 8, fontSize: 13, color: '#7fd4ff' },
  keyList: { display: 'flex', flexDirection: 'column', gap: 6, marginTop: 10 },
  keyItem: { display: 'flex', gap: 8, alignItems: 'center' },
  keyProvider: { fontSize: 13, fontWeight: 600, color: '#c8d6e5' },
  keyTag: { fontSize: 12, color: '#9aa7b3', background: '#10181f', border: '1px solid #24323e', borderRadius: 4, padding: '2px 8px' },
  levelRow: { display: 'flex', gap: 10, alignItems: 'center' },
  label: { fontSize: 13, color: '#c8d6e5' },
  hint: { marginTop: 14, fontSize: 12, color: '#5a6b7a' },
  empty: { color: '#5a6b7a', fontSize: 13 },
};
