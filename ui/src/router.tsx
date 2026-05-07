import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
} from "@tanstack/react-router";
import { TopNav } from "@/components/TopNav";
import { Chat } from "@/pages/Chat";
import { Home } from "@/pages/Home";
import { Manifest } from "@/pages/Manifest";
import { Models } from "@/pages/Models";

/**
 * Code-based TanStack Router setup. File-based routing (with the
 * `@tanstack/router-plugin` Vite plugin) is the canonical approach
 * but adds a build-time codegen step; for sub-phase 1d.1b's three
 * routes the code-based pattern is leaner. Revisit when the
 * route count grows past ~6–8 (likely v1.0.0 polish).
 */

const rootRoute = createRootRoute({
  component: RootLayout,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Home,
});

const manifestRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/manifest",
  component: Manifest,
});

const modelsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/models",
  component: Models,
});

const chatRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/chat",
  component: Chat,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  manifestRoute,
  modelsRoute,
  chatRoute,
]);

export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function RootLayout() {
  return (
    <>
      <TopNav />
      <main className="mx-auto max-w-3xl px-6 pt-12 pb-12">
        <Outlet />
      </main>
    </>
  );
}
