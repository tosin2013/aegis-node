import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * shadcn-style class-name composer. Combines `clsx` (conditional
 * class strings) with `tailwind-merge` (resolves conflicting
 * Tailwind utilities so the last one wins) so component variants
 * can extend without manually deduping classes.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
