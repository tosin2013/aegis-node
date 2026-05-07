import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ArrowDown,
  Bot,
  CircleAlert,
  MessageSquare,
  Send,
  ShieldCheck,
  User,
} from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  connectChatWs,
  createSession,
  type ChatWs,
  type ServerFrame,
} from "@/lib/chat-ws";

interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  /** Set on the assistant message that's currently streaming. */
  pending?: boolean;
  /** Set on assistant messages once `turn_end` arrives. The 1d.2c
   *  verifiable-badge surface uses this hook. */
  verifiable?: boolean;
}

type ConnState =
  | { kind: "connecting" }
  | { kind: "open"; sessionId: string }
  | { kind: "closed"; reason?: string }
  | { kind: "error"; message: string };

export function Chat() {
  const [conn, setConn] = useState<ConnState>({ kind: "connecting" });
  const [messages, setMessages] = useState<ChatMessage[]>([
    {
      id: "intro",
      role: "system",
      text: 'Welcome to the Aegis-Node chat surface (sub-phase 1d.2a). The backend is a stub — your prompts come back as "echo: …". Real Session::run_turn integration ships in 1d.2b.',
    },
  ]);
  const [input, setInput] = useState("");
  const wsRef = useRef<ChatWs | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  // Track which assistant turn is currently appending so streaming
  // chunks land on the same message bubble instead of creating new
  // ones. Cleared when turn_end fires.
  const activeTurnRef = useRef<string | null>(null);

  const handleFrame = useCallback((frame: ServerFrame) => {
    switch (frame.type) {
      case "turn_start": {
        const turnId = frame.turn_id;
        activeTurnRef.current = turnId;
        setMessages((prev) => [
          ...prev,
          {
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
            m.id === turnId ? { ...m, text: m.text + frame.text } : m,
          ),
        );
        break;
      }
      case "turn_end": {
        const turnId = frame.turn_id;
        activeTurnRef.current = null;
        setMessages((prev) =>
          prev.map((m) =>
            m.id === turnId
              ? { ...m, pending: false, verifiable: true }
              : m,
          ),
        );
        break;
      }
      case "error": {
        setMessages((prev) => [
          ...prev,
          {
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
  }, [handleFrame]);

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
              Stub backend (sub-phase 1d.2a) · real engine integration
              ships in 1d.2b
            </p>
          </div>
        </div>
        <ConnectionPill state={conn} />
      </header>

      <Card>
        <CardContent className="flex h-[560px] flex-col gap-4 p-0">
          <div
            ref={scrollRef}
            className="flex-1 overflow-y-auto px-6 pt-5 pb-2"
          >
            <div className="flex flex-col gap-4">
              {messages.map((m) => (
                <MessageBubble key={m.id} message={m} />
              ))}
            </div>
          </div>

          <div className="border-t border-[var(--color-border)] px-6 py-4">
            <div className="flex items-end gap-2">
              <textarea
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
                className="min-h-[44px] flex-1 resize-none rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-2 text-sm placeholder:text-muted focus:border-accent focus:outline-none disabled:opacity-50"
              />
              <button
                type="button"
                onClick={handleSend}
                disabled={!canSend}
                className="inline-flex h-11 items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] px-3 text-sm transition-colors hover:border-accent hover:text-accent disabled:opacity-50"
                aria-label="Send"
              >
                <Send className="h-4 w-4" aria-hidden="true" />
                <span>Send</span>
              </button>
            </div>
          </div>
        </CardContent>
      </Card>
    </>
  );
}

function MessageBubble({ message }: { message: ChatMessage }) {
  if (message.role === "system") {
    return (
      <div className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-4 py-3 text-xs text-muted">
        {message.text}
      </div>
    );
  }
  const isUser = message.role === "user";
  return (
    <div
      className={cn(
        "flex gap-3",
        isUser ? "flex-row-reverse" : "flex-row",
      )}
    >
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
        {message.verifiable && (
          <span
            className="inline-flex items-center gap-1 self-start font-mono text-[10px] text-muted"
            title="Sub-phase 1d.2c will hook this badge to the F9 ledger entry for this turn"
          >
            <ShieldCheck className="h-3 w-3" aria-hidden="true" />
            verifiable (1d.2c)
          </span>
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
