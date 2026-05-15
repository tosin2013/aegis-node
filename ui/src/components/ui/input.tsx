import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Input primitive per `docs/DESIGN.md` (`components.Input`).
 *
 * Sets `aria-invalid` styling so consumers can drive error visuals via
 * the standard ARIA attribute rather than a custom variant prop —
 * matches the chat surface's existing pattern of letting WS error
 * frames drive component state.
 */
export const Input = React.forwardRef<
  HTMLInputElement,
  React.InputHTMLAttributes<HTMLInputElement>
>(({ className, type, ...props }, ref) => (
  <input
    type={type}
    ref={ref}
    className={cn(
      "flex h-8 w-full min-w-0 rounded-md border border-[var(--color-border)]",
      "bg-[var(--color-bg-elev)] px-3 py-1 text-sm text-[var(--color-fg)]",
      "placeholder:text-muted",
      "transition-colors",
      "focus:border-[var(--color-border-strong)] focus:outline-none focus:ring-3 focus:ring-[color:var(--color-focus-ring)]/25",
      "disabled:cursor-not-allowed disabled:opacity-60",
      "aria-invalid:border-[var(--color-danger)] aria-invalid:focus:ring-[var(--color-danger)]/25",
      className,
    )}
    {...props}
  />
));
Input.displayName = "Input";
