import { Link, useRouterState } from "@tanstack/react-router";
import {
  Boxes,
  FileCode,
  Home as HomeIcon,
  MessageSquare,
  ShieldCheck,
} from "lucide-react";
import type { ComponentType, SVGProps } from "react";
import { cn } from "@/lib/utils";

interface NavItem {
  to: string;
  label: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
}

const NAV_ITEMS: NavItem[] = [
  { to: "/", label: "Home", icon: HomeIcon },
  { to: "/chat", label: "Chat", icon: MessageSquare },
  { to: "/manifest", label: "Manifest Builder", icon: FileCode },
  { to: "/models", label: "Model Library", icon: Boxes },
];

export function TopNav() {
  const router = useRouterState();
  const currentPath = router.location.pathname;

  return (
    <nav className="border-b border-[var(--color-border)] bg-[var(--color-bg)]">
      <div className="mx-auto flex max-w-3xl items-center justify-between px-6 py-2.5">
        <Link
          to="/"
          className="flex items-center gap-2 text-sm font-semibold tracking-tight transition-colors"
        >
          <ShieldCheck className="h-4 w-4 text-accent" aria-hidden="true" />
          <span>Aegis-Node</span>
        </Link>
        <ul className="flex items-center gap-0.5">
          {NAV_ITEMS.map((item) => {
            const active =
              item.to === "/"
                ? currentPath === "/"
                : currentPath.startsWith(item.to);
            return (
              <li key={item.to}>
                <Link
                  to={item.to}
                  className={cn(
                    "flex items-center gap-1.5 rounded-md px-2.5 py-1 text-sm transition-colors",
                    active
                      ? "bg-[var(--color-bg-elev)] text-[var(--color-fg)]"
                      : "text-muted hover:bg-[var(--color-bg-elev)] hover:text-[var(--color-fg)]",
                  )}
                  aria-current={active ? "page" : undefined}
                >
                  <item.icon
                    className="h-3.5 w-3.5"
                    aria-hidden="true"
                  />
                  <span>{item.label}</span>
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
    </nav>
  );
}
