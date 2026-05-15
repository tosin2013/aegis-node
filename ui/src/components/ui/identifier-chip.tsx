import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * IdentifierChip primitive per `docs/DESIGN.md` (`components.IdentifierChip`).
 *
 * Used for workload IDs, OCI digests, manifest hashes, SPIFFE URIs, F9
 * ledger entry IDs, and tool names. The chip treatment is the project's
 * answer to DESIGN.md's *Do*: "Render identifiers in mono with accent."
 * Per the Linear / Resend pattern, identifiers are rendered as filled
 * chips with a 1px ring — not as raw colored text inline with prose.
 *
 * When the identifier is truncated to fit, pair the chip with `<Tooltip>`
 * so the full bytes are recoverable on hover/focus. Truncation without
 * a tooltip is explicitly forbidden by DESIGN.md's *Don'ts*.
 */
export interface IdentifierChipProps
  extends React.HTMLAttributes<HTMLSpanElement> {
  /** Visual emphasis. `default` is accent-on-bg-elev; `muted` drops
   *  the accent color for places where the identifier is metadata,
   *  not the primary content (e.g. in a hover tooltip). */
  tone?: "default" | "muted";
}

export const IdentifierChip = React.forwardRef<
  HTMLSpanElement,
  IdentifierChipProps
>(({ className, tone = "default", children, ...props }, ref) => (
  <span
    ref={ref}
    className={cn(
      "inline-flex items-center gap-1 rounded-sm border border-[var(--color-border)]",
      "bg-[var(--color-bg-elev)] px-1.5 py-px",
      "font-mono text-[13px] font-medium leading-tight",
      "max-w-full truncate align-baseline",
      tone === "default" ? "text-accent" : "text-muted",
      className,
    )}
    {...props}
  >
    {children}
  </span>
));
IdentifierChip.displayName = "IdentifierChip";
