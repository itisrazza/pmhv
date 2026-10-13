import { Apple, Cpu, Layers, Server } from "lucide-react";
import type { Machine } from "../api/schemas";
import { archLabels, backendLabels } from "../lib/labels";

export function BackendIcon({ machine }: { machine: Machine }) {
  const label = backendLabels[machine.spec.backend];
  const Icon = machine.spec.backend === "vz" ? Apple : Server;
  return (
    <Icon size={16} aria-label={label}>
      <title>{label}</title>
    </Icon>
  );
}

/** Native and emulated machines get different icons, as in the mockups. */
export function PlatformIcon({ machine }: { machine: Machine }) {
  const arch = archLabels[machine.spec.arch];
  const label = machine.emulated ? `Emulated ${arch}` : `Native ${arch}`;
  const Icon = machine.emulated ? Layers : Cpu;
  return (
    <Icon size={16} aria-label={label}>
      <title>{label}</title>
    </Icon>
  );
}
