import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * shadcn-aligned Card primitives. Hand-authored for sub-phase
 * 1d.1a — the rest of the shadcn primitive set lands in 1d.1b
 * when the manifest builder + model library land. Theming is
 * driven by the design tokens in `src/index.css`.
 */

export const Card = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(
      // Flat by default — 1px hairline border replaces drop shadow
      // (Geist pattern). Surface differential against `bg` does the rest.
      "rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] text-[var(--color-fg)]",
      className,
    )}
    {...props}
  />
));
Card.displayName = "Card";

export const CardHeader = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("flex flex-col gap-1 px-5 pt-4 pb-3", className)}
    {...props}
  />
));
CardHeader.displayName = "CardHeader";

export const CardTitle = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <h2
    ref={ref}
    className={cn(
      "text-sm font-semibold tracking-tight text-[var(--color-fg)]",
      className,
    )}
    {...props}
  />
));
CardTitle.displayName = "CardTitle";

export const CardContent = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn("px-5 pb-5", className)} {...props} />
));
CardContent.displayName = "CardContent";
