import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Textarea primitive. Same skin as `<Input>` but for multi-line
 * content. Not enumerated in DESIGN.md `components.*` because the
 * spec follows the upstream google-labs-code/DESIGN.md schema which
 * doesn't have a Textarea entry — Textarea inherits Input's tokens
 * verbatim.
 *
 * Used by the chat composer; was an inline `<textarea>` before this
 * migration.
 */
export const Textarea = React.forwardRef<
  HTMLTextAreaElement,
  React.TextareaHTMLAttributes<HTMLTextAreaElement>
>(({ className, ...props }, ref) => (
  <textarea
    ref={ref}
    className={cn(
      "flex w-full min-w-0 rounded-md border border-[var(--color-border)]",
      "bg-[var(--color-bg-elev)] px-3 py-2 text-sm text-[var(--color-fg)]",
      "placeholder:text-muted",
      "resize-none transition-colors",
      "focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30",
      "disabled:cursor-not-allowed disabled:opacity-60",
      "aria-invalid:border-[var(--color-danger)] aria-invalid:focus:ring-[var(--color-danger)]/30",
      className,
    )}
    {...props}
  />
));
Textarea.displayName = "Textarea";
