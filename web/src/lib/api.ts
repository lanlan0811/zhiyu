//! Thin typed wrappers over the daemon JSON-RPC surface.

import type { DaemonClient } from './daemon';

export type Mode = 'daily' | 'coding';

export interface SessionRow {
  id: string;
  title: string;
  workspaceDir?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ModelConfig {
  id: string;
  vendor: string;
  name: string;
  baseUrl: string;
  apiFormat: 'chat' | 'responses';
  contextWindow: number;
  maxOutputTokens: number;
  reasoning: {
    enabled: boolean;
    defaultLevel: string;
    levels: { value: string; label: string; description: string }[];
    providerOptionsByLevel: Record<string, unknown>;
  };
  providerKeyId?: string | null;
}

export interface ContextUsage {
  usedTokens: number;
  sizeTokens: number;
  maxTokens: number;
  breakdown: Record<string, number>;
}

export interface SearchHit {
  docId: string;
  path: string;
  title: string;
  chunkIndex: number;
  heading?: string | null;
  content: string;
  score: number;
}

export interface KbDocument {
  id: string;
  path: string;
  title: string;
  chunkCount: number;
  createdAt: string;
}

/** All daemon commands used by the UI. */
export class DaemonApi {
  constructor(private client: DaemonClient) {}

  sessionList(mode: Mode) {
    return this.client.request('sessionList', { mode }) as Promise<SessionRow[]>;
  }
  sessionCreate(mode: Mode, title?: string, workspaceDir?: string) {
    return this.client.request('sessionCreate', { mode, title, workspaceDir }) as Promise<SessionRow>;
  }
  sessionDelete(sessionId: string) {
    return this.client.request('sessionDelete', { sessionId });
  }
  sessionSend(sessionId: string, text: string, thoughtLevel?: string) {
    return this.client.request('sessionSend', { sessionId, text, thoughtLevel });
  }
  sessionResume(sessionId: string, nextCursor: number) {
    return this.client.request('sessionResume', { cursor: { sessionId, nextCursor } });
  }
  sessionContextUsage(sessionId: string) {
    return this.client.request('sessionContextUsage', { sessionId }) as Promise<ContextUsage>;
  }
  sessionCompact(sessionId: string) {
    return this.client.request('sessionCompact', { sessionId });
  }
  sessionSetThoughtLevel(sessionId: string, level: string) {
    return this.client.request('sessionSetThoughtLevel', { sessionId, level });
  }

  modelList() {
    return this.client.request('modelList') as Promise<ModelConfig[]>;
  }
  modelSave(config: ModelConfig) {
    return this.client.request('modelSave', { config });
  }
  modelDelete(modelId: string) {
    return this.client.request('modelDelete', { modelId });
  }
  keyList(provider?: string) {
    return this.client.request('keyList', { provider });
  }
  keySave(provider: string, key: string) {
    return this.client.request('keySave', { provider, key });
  }
  modelSwitchGuard(sessionId: string, modelId: string) {
    return this.client.request('modelSwitchGuard', { sessionId, modelId });
  }

  knowledgeSearch(query: string, limit = 5) {
    return this.client.request('knowledgeSearch', { query, limit }) as Promise<SearchHit[]>;
  }
  knowledgeList() {
    return this.client.request('knowledgeList') as Promise<KbDocument[]>;
  }
  knowledgeImport(path: string) {
    return this.client.request('knowledgeImport', { path });
  }
  knowledgeDelete(docId: string) {
    return this.client.request('knowledgeDelete', { docId });
  }
  knowledgeReindex() {
    return this.client.request('knowledgeReindex');
  }

  workspaceListDir(sessionId: string, path?: string) {
    return this.client.request('workspaceListDir', { sessionId, path });
  }
  workspaceReadFile(sessionId: string, path: string) {
    return this.client.request('workspaceReadFile', { sessionId, path }) as Promise<{ content: string }>;
  }
  workspaceWriteFile(sessionId: string, path: string, content: string) {
    return this.client.request('workspaceWriteFile', { sessionId, path, content });
  }
  terminalExec(sessionId: string, command: string) {
    return this.client.request('terminalExec', { sessionId, command }) as Promise<{ output: string }>;
  }
  gitCheckpoint(sessionId: string, description?: string) {
    return this.client.request('gitCheckpoint', { sessionId, description });
  }
  gitRollback(sessionId: string, checkpointId: string) {
    return this.client.request('gitRollback', { sessionId, checkpointId });
  }

  browserExecute(sessionId: string, request: Record<string, unknown>) {
    return this.client.request('browserExecute', { sessionId, request });
  }

  writingRun(sessionId: string, task: { kind: string; topic: string; length?: string; language?: string }) {
    return this.client.request('writingRun', { sessionId, task });
  }

  settingsGet() {
    return this.client.request('settingsGet') as Promise<Record<string, unknown>>;
  }
  settingsSet(patch: Record<string, unknown>) {
    return this.client.request('settingsSet', { patch });
  }
}
