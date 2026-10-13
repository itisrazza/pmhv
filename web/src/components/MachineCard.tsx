import { Link } from "@tanstack/react-router";
import type { Machine } from "../api/schemas";
import { guestOsLabels } from "../lib/labels";
import { BackendIcon, PlatformIcon } from "./MachineIcons";
import { StateBadge } from "./StateBadge";
import { Thumbnail } from "./Thumbnail";

export function MachineCard({ machine }: { machine: Machine }) {
  return (
    <Link
      to="/machines/$id"
      params={{ id: machine.id }}
      className="flex flex-col gap-2 rounded-lg border border-border bg-card p-1.5 text-card-foreground hover:border-primary"
    >
      <div className="relative">
        <Thumbnail machine={machine} />
        <StateBadge
          state={machine.state}
          className="absolute bottom-2 left-2"
        />
      </div>
      <div className="flex flex-col gap-1 px-2 pb-2">
        <span className="truncate font-bold">{machine.spec.name}</span>
        <span className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <BackendIcon machine={machine} />
          <PlatformIcon machine={machine} />
          {guestOsLabels[machine.spec.guest_os]}
        </span>
      </div>
    </Link>
  );
}
