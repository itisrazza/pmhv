import { Monitor, SquareTerminal } from "lucide-react";
import type { Machine } from "../api/schemas";

/** The machine's screen as last captured, or a placeholder when there's nothing to show. */
export function Thumbnail({ machine }: { machine: Machine }) {
  const Icon = machine.spec.console === "graphical" ? Monitor : SquareTerminal;
  return (
    <div className="flex aspect-video w-full items-center justify-center overflow-hidden rounded-md bg-black">
      <Icon size={32} className="text-white/30" />
    </div>
  );
}
