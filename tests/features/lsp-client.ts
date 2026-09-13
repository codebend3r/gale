/**
 * A tiny JSON-RPC client for driving `gale --lsp` over stdio in tests.
 *
 * It speaks just enough of the Language Server Protocol to initialise, open a
 * document, wait for `textDocument/publishDiagnostics`, and issue requests.
 */

import { BASE_ENV, GALE_BIN } from "./helpers";

type Json = Record<string, unknown>;

interface Waiter {
  method: string;
  resolve: (params: Json) => void;
}

export class LspClient {
  private proc: ReturnType<typeof Bun.spawn>;
  private buffer = Buffer.alloc(0);
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: Json) => void; reject: (e: Error) => void }>();
  private notifications: { method: string; params: Json }[] = [];
  private waiters: Waiter[] = [];
  private closed = false;

  constructor(cwd: string, extraArgs: string[] = []) {
    this.proc = Bun.spawn([GALE_BIN, "--lsp", ...extraArgs], {
      cwd,
      stdin: "pipe",
      stdout: "pipe",
      stderr: "ignore",
      env: BASE_ENV,
    });
    void this.readLoop();
  }

  private async readLoop(): Promise<void> {
    const reader = (this.proc.stdout as ReadableStream<Uint8Array>).getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      this.buffer = Buffer.concat([this.buffer, Buffer.from(value)]);
      this.drain();
    }
  }

  private drain(): void {
    for (;;) {
      const headerEnd = this.buffer.indexOf("\r\n\r\n");
      if (headerEnd === -1) return;

      const header = this.buffer.subarray(0, headerEnd).toString("latin1");
      const match = /Content-Length:\s*(\d+)/i.exec(header);
      if (!match) {
        throw new Error(`LSP frame without Content-Length: ${header}`);
      }
      const length = Number(match[1]);
      const bodyStart = headerEnd + 4;
      if (this.buffer.length < bodyStart + length) return;

      const body = this.buffer.subarray(bodyStart, bodyStart + length).toString("utf8");
      this.buffer = this.buffer.subarray(bodyStart + length);
      this.dispatch(JSON.parse(body) as Json);
    }
  }

  private dispatch(message: Json): void {
    if (typeof message.id === "number" && ("result" in message || "error" in message)) {
      const entry = this.pending.get(message.id);
      if (!entry) return;
      this.pending.delete(message.id);
      if ("error" in message) {
        entry.reject(new Error(`LSP error: ${JSON.stringify(message.error)}`));
      } else {
        entry.resolve(message.result as Json);
      }
      return;
    }

    if (typeof message.method === "string") {
      const params = (message.params ?? {}) as Json;
      const index = this.waiters.findIndex((w) => w.method === message.method);
      if (index !== -1) {
        const [waiter] = this.waiters.splice(index, 1);
        waiter.resolve(params);
      } else {
        this.notifications.push({ method: message.method, params });
      }
    }
  }

  private send(message: Json): void {
    const body = Buffer.from(JSON.stringify(message), "utf8");
    const frame = Buffer.concat([
      Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, "latin1"),
      body,
    ]);
    const stdin = this.proc.stdin as { write(chunk: Uint8Array): unknown; flush?: () => void };
    stdin.write(frame);
    stdin.flush?.();
  }

  request<T = Json>(method: string, params: Json = {}): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: Json) => void, reject });
      this.send({ jsonrpc: "2.0", id, method, params });
      setTimeout(() => {
        if (this.pending.delete(id)) {
          reject(new Error(`LSP request ${method} timed out`));
        }
      }, 10_000);
    });
  }

  notify(method: string, params: Json = {}): void {
    this.send({ jsonrpc: "2.0", method, params });
  }

  /** Resolve with the params of the next notification named `method`. */
  waitFor<T = Json>(method: string, timeoutMs = 10_000): Promise<T> {
    const index = this.notifications.findIndex((n) => n.method === method);
    if (index !== -1) {
      const [found] = this.notifications.splice(index, 1);
      return Promise.resolve(found.params as T);
    }
    return new Promise<T>((resolve, reject) => {
      const waiter: Waiter = { method, resolve: resolve as (p: Json) => void };
      this.waiters.push(waiter);
      setTimeout(() => {
        const i = this.waiters.indexOf(waiter);
        if (i !== -1) {
          this.waiters.splice(i, 1);
          reject(new Error(`Timed out waiting for ${method}`));
        }
      }, timeoutMs);
    });
  }

  async initialize(rootDir: string): Promise<Json> {
    const result = await this.request("initialize", {
      processId: null,
      rootUri: `file://${rootDir}`,
      capabilities: {},
    });
    this.notify("initialized", {});
    return result;
  }

  /** Open a document and return the diagnostics the server publishes for it. */
  async open(uri: string, text: string, languageId = "css"): Promise<Json[]> {
    const published = this.waitFor<{ uri: string; diagnostics: Json[] }>(
      "textDocument/publishDiagnostics",
    );
    this.notify("textDocument/didOpen", {
      textDocument: { uri, languageId, version: 1, text },
    });
    return (await published).diagnostics;
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    try {
      await this.request("shutdown", {});
      this.notify("exit", {});
    } catch {
      // The server may already be gone.
    }
    this.proc.kill();
    await this.proc.exited;
  }
}
