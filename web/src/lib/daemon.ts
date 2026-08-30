//! WebSocket client for the local zhiyu daemon (JSON-RPC over WS).

export interface HelloReply {
  protocolVersion: number;
  serverName: string;
  lastSeq: number;
  now: string;
}

export type Event =
  | { type: 'message'; seq: number; message: unknown }
  | { type: 'text_delta'; seq: number; sessionId: string; delta: string }
  | { type: 'reasoning_delta'; seq: number; sessionId: string; delta: string }
  | { type: 'tool_started'; seq: number; sessionId: string; callId: string; name: string; args: unknown }
  | { type: 'tool_finished'; seq: number; sessionId: string; callId: string; ok: boolean; output: string }
  | { type: 'usage_update'; seq: number; sessionId: string; usage: unknown }
  | { type: 'turn_finished'; seq: number; sessionId: string; cursor: number }
  | { type: 'session_changed'; seq: number; sessionId: string; mode: string }
  | { type: 'status'; seq: number; sessionId: string | null; text: string }
  | { type: 'context_compacted'; seq: number; sessionId: string; trigger: string; preCompactTokens: number; postCompactTokens: number };

/** The daemon connection: hello handshake, request/response, event stream. */
export class DaemonClient {
  private ws: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private eventHandlers = new Set<(e: Event) => void>();
  private seq = 0;

  constructor(
    private url: string,
    private token: string,
  ) {}

  /** Connect + handshake. Resolves once the HelloReply arrives. */
  connect(): Promise<HelloReply> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      this.ws = ws;
      const timer = setTimeout(() => reject(new Error('handshake timeout')), 10_000);

      ws.onopen = () => {
        ws.send(
          JSON.stringify({
            protocolVersion: 1,
            token: this.token,
            clientName: 'zhiyu-web',
            lastSeq: this.seq,
          }),
        );
      };
      ws.onmessage = (ev) => {
        const data = JSON.parse(String(ev.data));
        if ('serverName' in data) {
          clearTimeout(timer);
          this.seq = data.lastSeq;
          resolve(data as HelloReply);
          return;
        }
        if ('id' in data && 'result' in data) {
          const p = this.pending.get(data.id);
          if (p) {
            this.pending.delete(data.id);
            if (data.error) p.reject(new Error(data.error.message));
            else p.resolve(data.result);
          }
          return;
        }
        if ('type' in data) {
          const ev = data as Event;
          if (ev.seq > this.seq) this.seq = ev.seq;
          this.eventHandlers.forEach((h) => h(ev));
        }
      };
      ws.onerror = () => reject(new Error('websocket error'));
      ws.onclose = () => {
        this.pending.forEach((p) => p.reject(new Error('connection closed')));
        this.pending.clear();
      };
    });
  }

  /** Sends a JSON-RPC request and awaits the response. */
  request(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error('not connected'));
        return;
      }
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, command: { method, ...params } }));
    });
  }

  onEvent(handler: (e: Event) => void): () => void {
    this.eventHandlers.add(handler);
    return () => this.eventHandlers.delete(handler);
  }

  close() {
    this.ws?.close();
    this.ws = null;
  }
}

/** Builds the ws URL for the loopback daemon. */
export function daemonUrl(port: number): string {
  const proto = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const host = window.location.hostname || '127.0.0.1';
  return `${proto}://${host}:${port}`;
}
