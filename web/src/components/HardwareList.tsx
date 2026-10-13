import type { ReactNode } from "react";
import type { Machine } from "../api/schemas";
import {
  archLabels,
  backendLabels,
  busLabels,
  consoleLabels,
  firmwareLabels,
  formatMemory,
  guestOsLabels,
  mediaLabels,
  networkLabels,
} from "../lib/labels";
import { BackendIcon, PlatformIcon } from "./MachineIcons";

interface Row {
  label: string;
  value: ReactNode;
}

function Rows({ rows }: { rows: Row[] }) {
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-8 gap-y-2">
      {rows.map(({ label, value }) => (
        <div key={label} className="contents">
          <dt className="font-semibold">{label}</dt>
          <dd className="flex items-center justify-end gap-1.5 text-right">
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

export function HardwareList({ machine }: { machine: Machine }) {
  const { spec } = machine;
  const busIndex = new Map<string, number>();
  const disks: Row[] = (spec.disks ?? []).map((disk) => {
    const index = busIndex.get(disk.bus) ?? 0;
    busIndex.set(disk.bus, index + 1);
    return {
      label: `${mediaLabels[disk.media]} / ${busLabels[disk.bus]} ${index}`,
      value: disk.image.name,
    };
  });

  return (
    <div className="flex flex-col gap-6">
      <Rows
        rows={[
          {
            label: "Hypervisor",
            value: (
              <>
                <BackendIcon machine={machine} />
                {backendLabels[spec.backend]}
              </>
            ),
          },
          {
            label: "Architecture",
            value: (
              <>
                <PlatformIcon machine={machine} />
                {machine.emulated ? "Emulated" : "Native"} (
                {archLabels[spec.arch]})
              </>
            ),
          },
          { label: "Firmware", value: firmwareLabels[spec.firmware] },
          { label: "Operating system", value: guestOsLabels[spec.guest_os] },
        ]}
      />
      <Rows
        rows={[
          { label: "CPU", value: `${spec.cpu_cores} cores` },
          { label: "Memory", value: formatMemory(spec.memory_mib) },
          { label: "TPM", value: spec.tpm ? "TPM 2.0" : "None" },
        ]}
      />
      {disks.length > 0 && <Rows rows={disks} />}
      <Rows
        rows={[
          { label: "Network", value: networkLabels[spec.network] },
          ...(spec.port_forwards ?? []).map((forward) => ({
            label: `Forward ${forward.protocol.toUpperCase()} ${forward.host_port}`,
            value: `127.0.0.1:${forward.host_port} → guest ${forward.guest_port}`,
          })),
          { label: "Console", value: consoleLabels[spec.console] },
        ]}
      />
    </div>
  );
}
