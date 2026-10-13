import { Outlet, useMatches } from "@tanstack/react-router";
import { PageHeader, Scaffold } from "@swopstar/react-ui";
import { AppSidebar } from "../components/AppSidebar";

export function AppLayout() {
  const matches = useMatches();
  const title = matches[matches.length - 1]?.staticData.title ?? "";

  return (
    <Scaffold sidebar={<AppSidebar />} header={<PageHeader title={title} />}>
      <Outlet />
    </Scaffold>
  );
}
