import type {
  Arch,
  Backend,
  ConsoleKind,
  DiskBus,
  DiskMedia,
  Firmware,
  GuestOs,
  MachineState,
  NetworkSpec,
} from "../api/schemas";

export const stateLabels: Record<MachineState, string> = {
  stopped: "Stopped",
  starting: "Starting",
  running: "Running",
  paused: "Paused",
  stopping: "Stopping",
};

export const guestOsLabels: Record<GuestOs, string> = {
  linux: "Linux",
  macos: "macOS",
  windows: "Windows",
  other: "Other",
};

export const backendLabels: Record<Backend, string> = {
  qemu: "QEMU",
  vz: "Apple Virtualization",
  dummy: "Dummy",
};

export const archLabels: Record<Arch, string> = {
  aarch64: "ARM64 / aarch64",
  x86_64: "x86-64",
};

export const firmwareLabels: Record<Firmware, string> = {
  uefi: "UEFI",
  bios: "BIOS",
};

export const consoleLabels: Record<ConsoleKind, string> = {
  text: "Text (serial)",
  graphical: "Graphical (VNC)",
};

export const busLabels: Record<DiskBus, string> = {
  virtio: "VirtIO",
  nvme: "NVMe",
  sata: "SATA",
  usb: "USB",
};

export const mediaLabels: Record<DiskMedia, string> = {
  disk: "Disk",
  cdrom: "CD-ROM",
};

export const networkLabels: Record<NetworkSpec, string> = {
  none: "None",
  nat: "Built-in NAT",
};

export function formatBytes(bytes: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

export function formatMemory(mib: number): string {
  return mib % 1024 === 0 ? `${mib / 1024} GiB` : `${mib} MiB`;
}
