import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  Bot,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  Hammer,
  HelpCircle,
  Lock,
  MessageSquare,
  Send,
  ShieldCheck,
  ShieldOff,
  User,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { ModelPicker } from "@/components/ModelPicker";
import {
  connectChatWs,
  createSession,
  type ChatWs,
  type ServerFrame,
  type ToolStatus,
} from "@/lib/chat-ws";

interface TextMessage {
  kind: "text";
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  /** Set on the assistant message that's currently streaming. */
  pending?: boolean;
  /** UUIDv7 of the F5 reasoning-step ledger entry, set by `turn_end`
   *  for backends that write to a ledger (real `Session::run_turn`,
   *  not the StubBackend). The verifiable badge tooltip shows the
   *  full anchor; click-through to a future `/replay/<anchor>` route
   *  is queued for ADR-010 viewer integration. */
  verifiableAnchor?: string;
}

interface ToolCallMessage {
  kind: "tool";
  id: string;
  turnId: string;
  name: string;
  args: unknown;
  /** `undefined` while the call is pending; set by the matching
   *  `tool_result` frame to one of the four mediator outcomes. */
  status?: ToolStatus;
  /** Mediator value on success. */
  value?: unknown;
  /** Reason text on denied / requires_approval / unroutable. */
  reason?: string;
}

type ChatMessage = TextMessage | ToolCallMessage;

type ConnState =
  | { kind: "connecting" }
  | { kind: "open"; sessionId: string }
  | { kind: "closed"; reason?: string }
  | { kind: "error"; message: string };

export function Chat() {
  const [conn, setConn] = useState<ConnState>({ kind: "connecting" });
  const [messages, setMessages] = useState<ChatMessage[]>([
    {
      kind: "text",
      id: "intro",
      role: "system",
      text: 'Welcome to the Aegis-Node chat surface. Without `--manifest`/`--model` the backend is a stub — your prompts come back as "echo: …". Start `aegis ui --manifest <m> --model <m>` to attach a real `Session::run_turn` against an inference backend.',
    },
  ]);
  const [input, setInput] = useState("");
  const wsRef = useRef<ChatWs | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  // Bumped after a successful Session Fork (ADR-032). The bootstrap
  // useEffect depends on this — bumping triggers WS teardown +
  // fresh `POST /api/v1/sessions` against the swapped backend, so
  // the new connection picks up the new model.
  const [sessionEpoch, setSessionEpoch] = useState(0);

  const handleFrame = useCallback((frame: ServerFrame) => {
    switch (frame.type) {
      case "turn_start": {
        const turnId = frame.turn_id;
        setMessages((prev) => [
          ...prev,
          {
            kind: "text",
            id: turnId,
            role: "assistant",
            text: "",
            pending: true,
          },
        ]);
        break;
      }
      case "assistant_text": {
        const turnId = frame.turn_id;
        setMessages((prev) =>
          prev.map((m) =>
            m.kind === "text" && m.id === turnId
              ? { ...m, text: m.text + frame.text }
              : m,
          ),
        );
        break;
      }
      case "tool_call": {
        // A new tool-call card lands here; status will flip when
        // the matching tool_result arrives.
        setMessages((prev) => [
          ...prev,
          {
            kind: "tool",
            id: `${frame.turn_id}:${frame.tool_call_id}`,
            turnId: frame.turn_id,
            name: frame.name,
            args: frame.args,
          },
        ]);
        break;
      }
      case "tool_result": {
        const id = `${frame.turn_id}:${frame.tool_call_id}`;
        setMessages((prev) =>
          prev.map((m) =>
            m.kind === "tool" && m.id === id
              ? {
                  ...m,
                  status: frame.status,
                  value: frame.value,
                  reason: frame.reason,
                }
              : m,
          ),
        );
        break;
      }
      case "turn_end": {
        const turnId = frame.turn_id;
        const anchor = frame.verifiable_anchor;
        setMessages((prev) =>
          prev.map((m) =>
            m.kind === "text" && m.id === turnId
              ? {
                  ...m,
                  pending: false,
                  verifiableAnchor: anchor,
                }
              : m,
          ),
        );
        break;
      }
      case "error": {
        setMessages((prev) => [
          ...prev,
          {
            kind: "text",
            id: `err-${Date.now()}`,
            role: "system",
            text: `error: ${frame.message}`,
          },
        ]);
        break;
      }
    }
  }, []);

  // Bootstrap: create a session, then open the WebSocket.
  useEffect(() => {
    let cancelled = false;
    let ws: ChatWs | null = null;

    (async () => {
      try {
        const session = await createSession();
        if (cancelled) return;
        ws = connectChatWs(session.session_id, {
          onOpen: () => {
            if (!cancelled) {
              setConn({ kind: "open", sessionId: session.session_id });
            }
          },
          onFrame: handleFrame,
          onClose: (code, reason) => {
            if (!cancelled) {
              setConn({
                kind: "closed",
                reason: reason || `code ${code}`,
              });
            }
          },
          onError: (e) => {
            if (!cancelled) {
              setConn({
                kind: "error",
                message: e instanceof Error ? e.message : String(e),
              });
            }
          },
        });
        wsRef.current = ws;
      } catch (e) {
        if (!cancelled) {
          setConn({
            kind: "error",
            message: e instanceof Error ? e.message : String(e),
          });
        }
      }
    })();

    return () => {
      cancelled = true;
      ws?.close();
      wsRef.current = null;
    };
  }, [handleFrame, sessionEpoch]);

  // Keep the message list scrolled to bottom on append.
  useEffect(() => {
    scrollRef.current?.scrollTo({
      top: scrollRef.current.scrollHeight,
      behavior: "smooth",
    });
  }, [messages]);

  const canSend = useMemo(
    () => conn.kind === "open" && input.trim().length > 0,
    [conn, input],
  );

  function handleSend() {
    if (!canSend || !wsRef.current) return;
    const prompt = input.trim();
    setMessages((prev) => [
      ...prev,
      {
        kind: "text",
        id: `user-${Date.now()}`,
        role: "user",
        text: prompt,
      },
    ]);
    wsRef.current.send({
      schema: "v1",
      type: "user_prompt",
      prompt,
    });
    setInput("");
  }

  // Called by ModelPicker after `POST /api/v1/sessions/fork` returns ok.
  // The backend has swapped the inner ChatBackend; we tear the WS down,
  // clear the chat thread (1d.2e.1 has no history replay — that's
  // 1d.2e.2), emit a system bookmark, and bump the epoch so the
  // bootstrap useEffect creates a fresh session against the new model.
  const handleForkComplete = useCallback((modelDigest: string) => {
    wsRef.current?.close();
    wsRef.current = null;
    const short = modelDigest.replace(/^sha256:/, "").slice(0, 8);
    setMessages([
      {
        kind: "text",
        id: `forked-${Date.now()}`,
        role: "system",
        text: `Session forked to model ${short}…. Chat history was cleared (replay lands in 1d.2e.2).`,
      },
    ]);
    setConn({ kind: "connecting" });
    setSessionEpoch((n) => n + 1);
  }, []);

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    // Enter sends; Shift+Enter inserts a newline (matches assistant-
    // ui / Claude / ChatGPT convention).
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  return (
    <>
      <header className="mb-6 flex items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <MessageSquare className="h-7 w-7 text-accent" aria-hidden="true" />
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">Chat</h1>
            <p className="text-sm text-muted">
              Inline tool-call cards render gate decisions per ADR-031 ·
              verifiable-badge wiring lands in 1d.2c.2
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <ModelPicker onForkComplete={handleForkComplete} />
          <ConnectionPill state={conn} />
        </div>
      </header>

      <Card>
        <CardContent className="flex h-[560px] flex-col gap-4 p-0">
          <div
            ref={scrollRef}
            className="flex-1 overflow-y-auto px-6 pt-5 pb-2"
          >
            <div className="flex flex-col gap-4">
              {messages.map((m) =>
                m.kind === "tool" ? (
                  <ToolCallCard key={m.id} call={m} />
                ) : (
                  <MessageBubble key={m.id} message={m} />
                ),
              )}
            </div>
          </div>

          <div className="border-t border-[var(--color-border)] px-6 py-4">
            <div className="flex items-end gap-2">
              <Textarea
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={
                  conn.kind === "open"
                    ? "Type a message · Enter sends, Shift+Enter for newline"
                    : "connecting…"
                }
                disabled={conn.kind !== "open"}
                rows={2}
                className="min-h-[44px] flex-1 bg-[var(--color-bg)]"
              />
              <Button
                type="button"
                variant="secondary"
                size="lg"
                onClick={handleSend}
                disabled={!canSend}
                aria-label="Send"
              >
                <Send aria-hidden="true" />
                <span>Send</span>
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
    </>
  );
}

function MessageBubble({ message }: { message: TextMessage }) {
  if (message.role === "system") {
    return (
      <div className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-4 py-3 text-xs text-muted">
        {message.text}
      </div>
    );
  }
  const isUser = message.role === "user";
  return (
    <div className={cn("flex gap-3", isUser ? "flex-row-reverse" : "flex-row")}>
      <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[var(--color-bg-elev)]">
        {isUser ? (
          <User className="h-4 w-4 text-muted" aria-hidden="true" />
        ) : (
          <Bot className="h-4 w-4 text-accent" aria-hidden="true" />
        )}
      </div>
      <div className="flex max-w-[80%] flex-col gap-1">
        <div
          className={cn(
            "whitespace-pre-wrap rounded-md px-3 py-2 text-sm",
            isUser
              ? "bg-[var(--color-bg-elev)] text-[var(--color-fg)]"
              : "bg-[var(--color-bg)] border border-[var(--color-border)] text-[var(--color-fg)]",
          )}
        >
          {message.text}
          {message.pending && (
            <span className="ml-1 inline-block h-3 w-3 animate-pulse rounded-full bg-accent align-middle" />
          )}
        </div>
        {message.verifiableAnchor && (
          <span
            className="inline-flex cursor-help items-center gap-1 self-start rounded bg-emerald-950/40 px-1.5 py-0.5 font-mono text-[10px] text-emerald-300"
            title={`F9 reasoning-step uuid: ${message.verifiableAnchor} — click-through to /replay/<anchor> lands when the ADR-010 viewer wires up`}
          >
            <ShieldCheck className="h-3 w-3" aria-hidden="true" />
            verifiable · {message.verifiableAnchor.slice(0, 8)}…
          </span>
        )}
      </div>
    </div>
  );
}

const TOOL_STATUS_META: Record<
  ToolStatus,
  {
    label: string;
    color: string;
    bg: string;
    Icon: typeof CircleAlert;
  }
> = {
  success: {
    label: "success",
    color: "text-emerald-300",
    bg: "bg-emerald-950/40",
    Icon: ShieldCheck,
  },
  denied: {
    label: "denied",
    color: "text-red-400",
    bg: "bg-red-950/40",
    Icon: ShieldOff,
  },
  requires_approval: {
    label: "approval required",
    color: "text-amber-300",
    bg: "bg-amber-950/40",
    Icon: Lock,
  },
  unroutable: {
    label: "unroutable",
    color: "text-sky-300",
    bg: "bg-sky-950/40",
    Icon: HelpCircle,
  },
};

function ToolCallCard({ call }: { call: ToolCallMessage }) {
  const [expanded, setExpanded] = useState(false);
  const meta = call.status ? TOOL_STATUS_META[call.status] : null;
  const StatusIcon = meta?.Icon;

  // The card layout intentionally mirrors the gate-decision pattern
  // ADR-031 §"Inline tool-call cards" calls out: name + status pill
  // top-line, expandable args + result detail. What makes Aegis-Node's
  // chat surface visibly different from a generic LLM front-end is
  // that every tool call carries the manifest's allow / deny / approve
  // verdict right next to it.
  const argsJson = useMemo(() => {
    try {
      return JSON.stringify(call.args, null, 2);
    } catch {
      return String(call.args);
    }
  }, [call.args]);

  const valueJson = useMemo(() => {
    if (call.value === undefined) return null;
    try {
      return JSON.stringify(call.value, null, 2);
    } catch {
      return String(call.value);
    }
  }, [call.value]);

  return (
    <div className="flex gap-3">
      <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-[var(--color-bg-elev)]">
        <Hammer className="h-4 w-4 text-accent" aria-hidden="true" />
      </div>
      <div className="flex max-w-[80%] flex-1 flex-col rounded-md border border-[var(--color-border)] bg-[var(--color-bg)]">
        <Button
          type="button"
          variant="ghost"
          onClick={() => setExpanded((e) => !e)}
          className="h-auto justify-between gap-3 rounded-none px-3 py-2 text-left text-sm font-normal text-[var(--color-fg)] hover:text-[var(--color-fg)]"
          aria-expanded={expanded}
        >
          <div className="flex items-center gap-2 truncate">
            {expanded ? (
              <ChevronDown
                className="h-3.5 w-3.5 shrink-0 text-muted"
                aria-hidden="true"
              />
            ) : (
              <ChevronRight
                className="h-3.5 w-3.5 shrink-0 text-muted"
                aria-hidden="true"
              />
            )}
            <span className="font-mono text-xs text-accent">{call.name}</span>
          </div>
          {meta ? (
            <span
              className={cn(
                "inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 font-mono text-[10px]",
                meta.bg,
                meta.color,
              )}
            >
              {StatusIcon && (
                <StatusIcon className="h-3 w-3" aria-hidden="true" />
              )}
              {meta.label}
            </span>
          ) : (
            <span className="inline-flex shrink-0 items-center gap-1 font-mono text-[10px] text-muted">
              <span className="inline-block h-2 w-2 animate-pulse rounded-full bg-accent" />
              dispatching…
            </span>
          )}
        </Button>
        {expanded && (
          <div className="border-t border-[var(--color-border)] px-3 py-2 text-xs">
            <div className="mb-2">
              <div className="mb-1 font-mono text-[10px] uppercase tracking-wider text-muted">
                args
              </div>
              <pre className="overflow-x-auto rounded bg-[var(--color-bg-elev)] p-2 font-mono text-[11px] text-[var(--color-fg)]">
                {argsJson}
              </pre>
            </div>
            {call.reason && (
              <div className="mb-2">
                <div className="mb-1 font-mono text-[10px] uppercase tracking-wider text-muted">
                  reason
                </div>
                <p className="font-mono text-[11px] text-[var(--color-fg)]">
                  {call.reason}
                </p>
              </div>
            )}
            {valueJson && (
              <div>
                <div className="mb-1 font-mono text-[10px] uppercase tracking-wider text-muted">
                  result
                </div>
                <pre className="overflow-x-auto rounded bg-[var(--color-bg-elev)] p-2 font-mono text-[11px] text-[var(--color-fg)]">
                  {valueJson}
                </pre>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ConnectionPill({ state }: { state: ConnState }) {
  if (state.kind === "connecting") {
    return (
      <span className="inline-flex items-center gap-1 font-mono text-xs text-muted">
        <ArrowDown className="h-3.5 w-3.5 animate-pulse" aria-hidden="true" />
        connecting…
      </span>
    );
  }
  if (state.kind === "open") {
    return (
      <span
        className="inline-flex items-center gap-1 rounded bg-emerald-950/40 px-2 py-0.5 font-mono text-xs text-emerald-300"
        title={`session ${state.sessionId}`}
      >
        <ShieldCheck className="h-3.5 w-3.5" aria-hidden="true" />
        connected
      </span>
    );
  }
  if (state.kind === "closed") {
    return (
      <span className="inline-flex items-center gap-1 font-mono text-xs text-amber-300">
        <CircleAlert className="h-3.5 w-3.5" aria-hidden="true" />
        closed{state.reason ? ` · ${state.reason}` : ""}
      </span>
    );
  }
  return (
    <span
      className="inline-flex items-center gap-1 font-mono text-xs text-red-400"
      title={state.message}
    >
      <CircleAlert className="h-3.5 w-3.5" aria-hidden="true" />
      error
    </span>
  );
}
