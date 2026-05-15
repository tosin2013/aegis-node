import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

/**
 * Button primitive per `docs/DESIGN.md` (`components.Button`).
 * Variants and sizing derive from the design spec; do not hand-edit
 * the values here — update DESIGN.md, regenerate tokens, then match.
 *
 * `asChild` swaps the rendered element for a React Slot so the
 * button styling can sit on top of e.g. a router `<Link>`.
 */
const buttonVariants = cva(
  cn(
    "inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap",
    "rounded-md font-medium transition-colors",
    "focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-[color:var(--color-focus-ring)]/25",
    "disabled:pointer-events-none disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0",
  ),
  {
    variants: {
      // `default` is the inverted-neutral primary (Linear/Geist pattern).
      // Accent is reserved for identifier emphasis + focus rings —
      // intentionally NOT used as a button fill.
      variant: {
        default:
          "bg-[var(--color-fg)] text-[var(--color-bg)] hover:opacity-90 active:opacity-82",
        secondary:
          "bg-[var(--color-bg-elev)] text-[var(--color-fg)] border border-[var(--color-border)] hover:border-[var(--color-border-strong)] hover:bg-[var(--color-bg)]",
        ghost:
          "bg-transparent text-muted hover:bg-[var(--color-bg-elev)] hover:text-[var(--color-fg)]",
        danger:
          "bg-[var(--color-danger)] text-[var(--color-bg)] hover:opacity-90 active:opacity-82",
      },
      size: {
        sm: "h-7 px-2 text-xs [&_svg]:h-3.5 [&_svg]:w-3.5",
        md: "h-8 px-3 text-sm [&_svg]:h-3.5 [&_svg]:w-3.5",
        lg: "h-9 px-3.5 text-sm [&_svg]:h-4 [&_svg]:w-4",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "md",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        ref={ref}
        className={cn(buttonVariants({ variant, size }), className)}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { buttonVariants };
