/**
 * Chat WebSocket client. Wraps the protocol that
 * `crates/ui-server/src/handlers/sessions.rs` exposes:
 *
 *   POST /api/v1/sessions     → { session_id }
 *   WS   /api/v1/stream?sid=X → bidirectional v1 frames
 *
 * Schema-versioned frames keep the door open for sub-phase 1d.2c/d
 * additions (tool_call, tool_result, verifiable badge metadata)
 * without breaking older clients — the type union below is an
 * open-ended discriminated union, server-emitted frames the SPA
 * doesn't know about are surfaced via `onUnknown` rather than
 * crashing the connection.
 */

export interface SessionCreated {
  session_id: string;
  created_at: string;
  schema: string;
}

/** Mediator's terminal decision for one tool call. Mirrors the
 *  Rust enum `TurnToolCallStatus` — the four reachable outcomes from
 *  `aegis_inference_engine::ToolCallResult`. */
export type ToolStatus =
  | "success"
  | "denied"
  | "requires_approval"
  | "unroutable";

export type ServerFrame =
  | { schema: "v1"; type: "turn_start"; turn_id: string }
  | {
      schema: "v1";
      type: "assistant_text";
      turn_id: string;
      text: string;
    }
  | {
      schema: "v1";
      type: "tool_call";
      turn_id: string;
      tool_call_id: string;
      name: string;
      args: unknown;
    }
  | {
      schema: "v1";
      type: "tool_result";
      turn_id: string;
      tool_call_id: string;
      status: ToolStatus;
      value?: unknown;
      reason?: string;
    }
  | { schema: "v1"; type: "turn_end"; turn_id: string }
  | { schema: "v1"; type: "error"; message: string };

export type ClientFrame = {
  schema: "v1";
  type: "user_prompt";
  prompt: string;
};

export interface ChatWsHandlers {
  onFrame: (frame: ServerFrame) => void;
  onUnknown?: (raw: unknown) => void;
  onOpen?: () => void;
  onClose?: (code: number, reason: string) => void;
  onError?: (err: unknown) => void;
}

export interface ChatWs {
  send: (frame: ClientFrame) => void;
  close: () => void;
  readyState: () => number;
}

export async function createSession(): Promise<SessionCreated> {
  const r = await fetch("/api/v1/sessions", { method: "POST" });
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    throw new Error(`POST /api/v1/sessions failed: ${r.status} ${body}`);
  }
  return (await r.json()) as SessionCreated;
}

export function connectChatWs(
  sessionId: string,
  handlers: ChatWsHandlers,
): ChatWs {
  // Same-origin WebSocket — derive ws:// or wss:// from the page's
  // protocol so HTTPS deployments (none today, but coming once the
  // localhost UI moves behind reverse-proxied TLS for ops use) are
  // automatic.
  const proto = window.location.protocol === "https:" ? "wss" : "ws";
  const url = `${proto}://${window.location.host}/api/v1/stream?sid=${encodeURIComponent(sessionId)}`;
  const ws = new WebSocket(url);

  ws.addEventListener("open", () => handlers.onOpen?.());
  ws.addEventListener("close", (ev) =>
    handlers.onClose?.(ev.code, ev.reason),
  );
  ws.addEventListener("error", (ev) => handlers.onError?.(ev));
  ws.addEventListener("message", (ev) => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(ev.data as string);
    } catch (e) {
      handlers.onError?.(e);
      return;
    }
    if (isServerFrame(parsed)) {
      handlers.onFrame(parsed);
    } else {
      handlers.onUnknown?.(parsed);
    }
  });

  return {
    send: (frame: ClientFrame) => {
      ws.send(JSON.stringify(frame));
    },
    close: () => ws.close(),
    readyState: () => ws.readyState,
  };
}

const TOOL_STATUSES: ReadonlySet<string> = new Set([
  "success",
  "denied",
  "requires_approval",
  "unroutable",
]);

function isServerFrame(v: unknown): v is ServerFrame {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  if (o.schema !== "v1" || typeof o.type !== "string") return false;
  switch (o.type) {
    case "turn_start":
    case "turn_end":
      return typeof o.turn_id === "string";
    case "assistant_text":
      return typeof o.turn_id === "string" && typeof o.text === "string";
    case "tool_call":
      return (
        typeof o.turn_id === "string" &&
        typeof o.tool_call_id === "string" &&
        typeof o.name === "string"
      );
    case "tool_result":
      return (
        typeof o.turn_id === "string" &&
        typeof o.tool_call_id === "string" &&
        typeof o.status === "string" &&
        TOOL_STATUSES.has(o.status)
      );
    case "error":
      return typeof o.message === "string";
    default:
      return false;
  }
}
