import { Link } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { getGetSessionQueryKey, useLogout } from "../api/auth/auth";
import { useListMachines } from "../api/machines/machines";
import { useFavourites } from "../lib/favourites";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarSeparator,
} from "@swopstar/react-ui";
import {
  Box,
  FileBox,
  LogOut,
  Network,
  Settings,
  type LucideIcon,
} from "lucide-react";

interface NavItem {
  to: "/machines" | "/images" | "/networks" | "/settings";
  label: string;
  icon: LucideIcon;
}

const mainItems: readonly NavItem[] = [
  { to: "/machines", label: "Machines", icon: Box },
  { to: "/images", label: "Disk images", icon: FileBox },
  { to: "/networks", label: "Networks", icon: Network },
];

const footerItems: readonly NavItem[] = [
  { to: "/settings", label: "Settings", icon: Settings },
];

function NavMenu({ items }: { items: readonly NavItem[] }) {
  return (
    <SidebarMenu>
      {items.map(({ to, label, icon: Icon }) => (
        <SidebarMenuItem key={to}>
          <SidebarMenuButton asChild>
            <Link to={to} activeProps={{ "data-active": "true" }}>
              <Icon size={16} />
              {label}
            </Link>
          </SidebarMenuButton>
        </SidebarMenuItem>
      ))}
    </SidebarMenu>
  );
}

function FavouriteMachines() {
  const { favourites } = useFavourites();
  const machines = useListMachines();
  const byId = new Map(machines.data?.map((machine) => [machine.id, machine]));
  const pinned = favourites.flatMap((id) => byId.get(id) ?? []);
  if (pinned.length === 0) {
    return (
      <p className="px-2 text-xs text-muted-foreground">
        Star a machine to pin it here.
      </p>
    );
  }
  return (
    <SidebarMenu>
      {pinned.map((machine) => (
        <SidebarMenuItem key={machine.id}>
          <SidebarMenuButton asChild>
            <Link
              to="/machines/$id"
              params={{ id: machine.id }}
              activeProps={{ "data-active": "true" }}
            >
              <Box size={16} />
              {machine.spec.name}
            </Link>
          </SidebarMenuButton>
        </SidebarMenuItem>
      ))}
    </SidebarMenu>
  );
}

function DisconnectButton() {
  const queryClient = useQueryClient();
  const logout = useLogout({
    mutation: {
      onSettled: () => {
        queryClient.clear();
        void queryClient.invalidateQueries({
          queryKey: getGetSessionQueryKey(),
        });
      },
    },
  });
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => logout.mutate()}
        disabled={logout.isPending}
      >
        <LogOut size={16} />
        Disconnect
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

export function AppSidebar() {
  return (
    <Sidebar>
      <SidebarHeader>
        <div className="flex items-center gap-2 p-1">
          <img src="/favicon.svg" width={28} height={28} alt="" />
          <span className="text-lg font-extrabold">pmhv</span>
        </div>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <NavMenu items={mainItems} />
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarGroupLabel>Favourites</SidebarGroupLabel>
          <SidebarGroupContent>
            <FavouriteMachines />
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      <SidebarSeparator />

      <SidebarFooter>
        <NavMenu items={footerItems} />
        <SidebarMenu>
          <DisconnectButton />
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
  );
}
