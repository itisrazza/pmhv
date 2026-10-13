import { cn } from "@swopstar/react-ui";
import type { MachineState } from "../api/schemas";
import { stateLabels } from "../lib/labels";

const stateStyles: Record<MachineState, string> = {
  running: "bg-positive text-positive-foreground border-positive-edge",
  paused: "bg-warning text-warning-foreground border-warning-edge",
  starting: "bg-secondary text-secondary-foreground border-border",
  stopping: "bg-secondary text-secondary-foreground border-border",
  stopped: "bg-background text-foreground border-border",
};

export function StateBadge({
  state,
  className,
}: {
  state: MachineState;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-md border px-2 py-0.5 text-sm font-medium",
        stateStyles[state],
        className,
      )}
    >
      {stateLabels[state]}
    </span>
  );
}
