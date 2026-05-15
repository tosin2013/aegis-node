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
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40",
    "disabled:pointer-events-none disabled:opacity-50",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0",
  ),
  {
    variants: {
      variant: {
        default:
          "bg-accent text-[var(--color-bg)] hover:opacity-90 active:opacity-80",
        secondary:
          "bg-[var(--color-bg-elev)] text-[var(--color-fg)] border border-[var(--color-border)] hover:border-accent hover:text-accent",
        ghost:
          "bg-transparent text-muted hover:bg-[var(--color-bg-elev)] hover:text-[var(--color-fg)]",
        danger:
          "bg-[var(--color-danger)] text-[var(--color-bg)] hover:opacity-90 active:opacity-80",
      },
      size: {
        sm: "h-8 px-2 text-xs [&_svg]:h-3.5 [&_svg]:w-3.5",
        md: "h-9 px-3 text-sm [&_svg]:h-4 [&_svg]:w-4",
        lg: "h-11 px-4 text-base [&_svg]:h-5 [&_svg]:w-5",
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
