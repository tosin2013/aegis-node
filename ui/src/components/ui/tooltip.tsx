import * as React from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cn } from "@/lib/utils";

/**
 * Tooltip primitive per `docs/DESIGN.md` — built on Radix Tooltip.
 *
 * Anchor use case is identifier-on-hover: workload IDs, OCI digests,
 * manifest hashes, SPIFFE URIs are shown shortened in the UI but the
 * full value pops on hover/focus. Per DESIGN.md's *Don'ts*: don't
 * drop bytes from an identifier without an affordance to recover
 * them — Tooltip is the affordance.
 *
 * Wrap your app once in `<TooltipProvider>` to share the
 * delayDuration across all tooltips; individual `<Tooltip>` instances
 * compose `Trigger` + `Content`.
 */

export const TooltipProvider = TooltipPrimitive.Provider;
export const Tooltip = TooltipPrimitive.Root;
export const TooltipTrigger = TooltipPrimitive.Trigger;

export const TooltipContent = React.forwardRef<
  React.ComponentRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ className, sideOffset = 6, ...props }, ref) => (
  <TooltipPrimitive.Portal>
    <TooltipPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      className={cn(
        "z-50 max-w-xs rounded-md border border-[var(--color-border)]",
        "bg-[var(--color-bg-elev)] px-2.5 py-1.5 text-xs text-[var(--color-fg)]",
        "shadow-md",
        "data-[state=delayed-open]:animate-in data-[state=closed]:animate-out",
        "data-[state=delayed-open]:fade-in-0 data-[state=closed]:fade-out-0",
        className,
      )}
      {...props}
    />
  </TooltipPrimitive.Portal>
));
TooltipContent.displayName = TooltipPrimitive.Content.displayName;
