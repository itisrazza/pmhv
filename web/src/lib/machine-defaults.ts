import type {
  Arch,
  ConsoleKind,
  CreateMachineRequest,
  DiskBus,
  DiskSpec,
  GuestOs,
} from "../api/schemas";

/** What the create dialog collects; turned into a request by `toRequest`. */
export interface MachineDraft {
  name: string;
  guestOs: GuestOs;
  arch: Arch;
  bios: boolean;
  cpuCores: number;
  memoryGib: number;
  bootDisk: "new" | "existing" | "none";
  newDiskGib: number;
  existingDisk: string;
  installMedia: string;
  console: ConsoleKind;
  tpm: boolean;
  notes: string;
}

const memoryGib: Record<GuestOs, number> = {
  linux: 2,
  macos: 4,
  windows: 4,
  other: 1,
};

const diskGib: Record<GuestOs, number> = {
  linux: 32,
  macos: 64,
  windows: 64,
  other: 16,
};

/** Defaults that follow from the guest OS; everything stays editable afterwards. */
export function defaultsFor(guestOs: GuestOs) {
  return {
    memoryGib: memoryGib[guestOs],
    newDiskGib: diskGib[guestOs],
    console: (guestOs === "linux" ? "text" : "graphical") as ConsoleKind,
    tpm: guestOs === "windows",
  };
}

export function initialDraft(hostArch: Arch): MachineDraft {
  return {
    name: "",
    guestOs: "linux",
    arch: hostArch,
    bios: false,
    cpuCores: 2,
    bootDisk: "new",
    existingDisk: "",
    installMedia: "",
    notes: "",
    ...defaultsFor("linux"),
  };
}

/** Disk bus for a new boot disk: VirtIO everywhere except emulated legacy guests. */
function bootBus(draft: MachineDraft): DiskBus {
  return draft.bios ? "sata" : "virtio";
}

/** CD-ROMs go on VirtIO-SCSI for ARM (EDK2 supports it there) and SATA for x86. */
function mediaBus(arch: Arch): DiskBus {
  return arch === "aarch64" ? "virtio" : "sata";
}

/** Images are referred to as `group/name` in the dialog's selects. */
function imageRef(key: string) {
  const separator = key.indexOf("/");
  return { group: key.slice(0, separator), name: key.slice(separator + 1) };
}

export function toRequest(draft: MachineDraft): CreateMachineRequest {
  const disks: DiskSpec[] = [];
  if (draft.bootDisk === "existing" && draft.existingDisk) {
    disks.push({
      image: imageRef(draft.existingDisk),
      bus: bootBus(draft),
      media: "disk",
    });
  }
  if (draft.installMedia) {
    disks.push({
      image: imageRef(draft.installMedia),
      bus: mediaBus(draft.arch),
      media: "cdrom",
    });
  }
  return {
    spec: {
      name: draft.name.trim(),
      guest_os: draft.guestOs,
      // The Virtualization.framework backend (for macOS guests) isn't implemented yet.
      backend: "qemu",
      arch: draft.arch,
      firmware: draft.bios ? "bios" : "uefi",
      cpu_cores: draft.cpuCores,
      memory_mib: Math.round(draft.memoryGib * 1024),
      disks,
      network: "nat",
      console: draft.console,
      tpm: draft.tpm,
      notes: draft.notes,
    },
    new_boot_disk:
      draft.bootDisk === "new"
        ? { size_gib: draft.newDiskGib, bus: bootBus(draft) }
        : null,
  };
}
