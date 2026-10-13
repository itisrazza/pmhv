import { Link } from "@tanstack/react-router";
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
import { Box, FileBox, Network, Settings, type LucideIcon } from "lucide-react";

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
            <SidebarMenu />
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      <SidebarSeparator />

      <SidebarFooter>
        <NavMenu items={footerItems} />
      </SidebarFooter>
    </Sidebar>
  );
}
