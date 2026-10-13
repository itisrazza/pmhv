import { lazy, Suspense } from "react";
import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";
import { Toaster } from "@swopstar/react-ui";
import { AppLayout } from "./layouts/AppLayout";
import { AuthGate } from "./components/AuthGate";
import { DiskImagesPage } from "./pages/DiskImagesPage";
import { MachinePage } from "./pages/MachinePage";
import { MachinesPage } from "./pages/MachinesPage";
import { NetworksPage } from "./pages/NetworksPage";
import { SettingsPage } from "./pages/SettingsPage";

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function Root() {
  return (
    <>
      <AuthGate>
        <Outlet />
      </AuthGate>
      <Toaster />
    </>
  );
}

const rootRoute = createRootRoute({ component: Root });

// noVNC and xterm.js are only needed in the console tab.
const ConsolePage = lazy(() =>
  import("./pages/ConsolePage").then((module) => ({
    default: module.ConsolePage,
  })),
);

const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "app",
  component: AppLayout,
});

const indexRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/machines" });
  },
});

const machinesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/machines",
  component: MachinesPage,
});

const machineRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/machines/$id",
  component: function MachineRoute() {
    const { id } = machineRoute.useParams();
    return <MachinePage id={id} />;
  },
});

const diskImagesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/images",
  component: DiskImagesPage,
});

const networksRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/networks",
  component: NetworksPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: "/settings",
  component: SettingsPage,
});

// The console opens in its own tab, so it gets the whole window rather than the app layout.
const consoleRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/machines/$id/console",
  component: function ConsoleRoute() {
    const { id } = consoleRoute.useParams();
    return (
      <Suspense>
        <ConsolePage id={id} />
      </Suspense>
    );
  },
});

const routeTree = rootRoute.addChildren([
  appRoute.addChildren([
    indexRoute,
    machinesRoute,
    machineRoute,
    diskImagesRoute,
    networksRoute,
    settingsRoute,
  ]),
  consoleRoute,
]);

export const router = createRouter({ routeTree });
