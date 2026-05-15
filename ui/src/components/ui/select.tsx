import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Select primitive per `docs/DESIGN.md` (`components.Select`).
 *
 * Native `<select>` styled to the design tokens. Wrapped in a
 * positioned `<div>` so the chevron icon can sit on top without
 * intercepting clicks. Native is chosen over Radix `<Select>` for
 * the a11y baseline (keyboard nav, screen-reader option enumeration,
 * mobile platform pickers) at the cost of custom option rendering —
 * a Radix variant lands when an existing consumer actually needs it
 * (e.g. icons in options).
 */
export const Select = React.forwardRef<
  HTMLSelectElement,
  React.SelectHTMLAttributes<HTMLSelectElement>
>(({ className, children, ...props }, ref) => (
  <div className="relative inline-flex w-full">
    <select
      ref={ref}
      className={cn(
        "flex h-9 w-full min-w-0 appearance-none rounded-md",
        "border border-[var(--color-border)]",
        "bg-[var(--color-bg-elev)] pr-8 pl-3 text-sm text-[var(--color-fg)]",
        "transition-colors",
        "focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30",
        "disabled:cursor-not-allowed disabled:opacity-60",
        "aria-invalid:border-[var(--color-danger)] aria-invalid:focus:ring-[var(--color-danger)]/30",
        className,
      )}
      {...props}
    >
      {children}
    </select>
    <ChevronDown
      className="pointer-events-none absolute top-1/2 right-2 h-4 w-4 -translate-y-1/2 text-muted"
      aria-hidden="true"
    />
  </div>
));
Select.displayName = "Select";
