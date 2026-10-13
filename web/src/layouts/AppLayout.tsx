import { useState } from "react";
import { Outlet } from "@tanstack/react-router";
import { Scaffold } from "@swopstar/react-ui";
import { AppSidebar } from "../components/AppSidebar";
import { PageHeaderSlotContext } from "../components/PageHeaderSlot";

export function AppLayout() {
  const [headerSlot, setHeaderSlot] = useState<HTMLElement | null>(null);

  return (
    <PageHeaderSlotContext.Provider value={headerSlot}>
      <Scaffold
        sidebar={<AppSidebar />}
        header={<div ref={setHeaderSlot} className="flex flex-1" />}
      >
        <Outlet />
      </Scaffold>
    </PageHeaderSlotContext.Provider>
  );
}
