import {
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";
import { AppLayout } from "./layouts/AppLayout";
import { DiskImagesPage } from "./pages/DiskImagesPage";
import { MachinesPage } from "./pages/MachinesPage";
import { NetworksPage } from "./pages/NetworksPage";
import { SettingsPage } from "./pages/SettingsPage";

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
  interface StaticDataRouteOption {
    title?: string;
  }
}

const rootRoute = createRootRoute({ component: AppLayout });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/machines" });
  },
});

const machinesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/machines",
  staticData: { title: "Machines" },
  component: MachinesPage,
});

const diskImagesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/images",
  staticData: { title: "Disk images" },
  component: DiskImagesPage,
});

const networksRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/networks",
  staticData: { title: "Networks" },
  component: NetworksPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  staticData: { title: "Settings" },
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  machinesRoute,
  diskImagesRoute,
  networksRoute,
  settingsRoute,
]);

export const router = createRouter({ routeTree });
