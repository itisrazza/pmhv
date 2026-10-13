import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import {
  Alert,
  AlertDescription,
  Button,
  Checkbox,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Textarea,
} from "@swopstar/react-ui";
import { useGetHost } from "../api/host/host";
import { useListImages } from "../api/images/images";
import {
  getListMachinesQueryKey,
  useCreateMachine,
} from "../api/machines/machines";
import type { Arch, ConsoleKind, GuestOs, ImageInfo } from "../api/schemas";
import { archLabels, consoleLabels, formatBytes } from "../lib/labels";
import {
  defaultsFor,
  initialDraft,
  supportsVz,
  toRequest,
  type MachineDraft,
} from "../lib/machine-defaults";
import { ChoiceSelect, type Choice } from "./form/ChoiceSelect";
import { Field } from "./form/Field";

const NONE = "none";

const guestOsChoices: Choice<GuestOs>[] = [
  { value: "linux", label: "Linux" },
  { value: "windows", label: "Windows" },
  { value: "other", label: "Other" },
  {
    value: "macos",
    label: "macOS (needs Apple Virtualization, coming later)",
    disabled: true,
  },
];

const consoleChoices: Choice<ConsoleKind>[] = [
  { value: "text", label: consoleLabels.text },
  { value: "graphical", label: consoleLabels.graphical },
];

function archChoices(hostArch: Arch): Choice<Arch>[] {
  return (["aarch64", "x86_64"] as const).map((arch) => ({
    value: arch,
    label:
      arch === hostArch
        ? `${archLabels[arch]} (native)`
        : `${archLabels[arch]} (emulated, slow)`,
  }));
}

function imageChoices(
  images: ImageInfo[],
  keep: (image: ImageInfo) => boolean,
): Choice<string>[] {
  return [
    { value: NONE, label: "None" },
    ...images.filter(keep).map((image) => ({
      value: `${image.group}/${image.name}`,
      label: `${image.name} (${formatBytes(image.virtual_bytes)})`,
    })),
  ];
}

interface CreateMachineDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CreateMachineDialog({
  open,
  onOpenChange,
}: CreateMachineDialogProps) {
  const host = useGetHost();
  if (!host.data) {
    return null;
  }
  const hostArch: Arch = host.data.arch === "x86_64" ? "x86_64" : "aarch64";
  const vzAvailable = host.data.os === "macos";
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && (
        <CreateMachineForm
          hostArch={hostArch}
          vzAvailable={vzAvailable}
          onDone={() => onOpenChange(false)}
        />
      )}
    </Dialog>
  );
}

function CreateMachineForm({
  hostArch,
  vzAvailable,
  onDone,
}: {
  hostArch: Arch;
  vzAvailable: boolean;
  onDone: () => void;
}) {
  const [draft, setDraft] = useState<MachineDraft>(() =>
    initialDraft(hostArch),
  );
  const images = useListImages();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const create = useCreateMachine({
    mutation: {
      onSuccess: (machine) => {
        void queryClient.invalidateQueries({
          queryKey: getListMachinesQueryKey(),
        });
        onDone();
        void navigate({ to: "/machines/$id", params: { id: machine.id } });
      },
    },
  });

  const update = (changes: Partial<MachineDraft>) =>
    setDraft((current) => {
      const next = { ...current, ...changes };
      // Apple Virtualization: native architecture, UEFI, text console, no TPM.
      return next.backend === "vz"
        ? {
            ...next,
            arch: hostArch,
            bios: false,
            console: "text",
            tpm: false,
          }
        : next;
    });
  const vz = draft.backend === "vz";
  const allImages = images.data?.flatMap((group) => group.images) ?? [];
  const diskChoices = imageChoices(
    allImages,
    (image) => image.format !== "iso",
  );
  const mediaChoices = imageChoices(
    allImages,
    (image) => image.format === "iso",
  );

  return (
    <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>New machine</DialogTitle>
        <DialogDescription>
          Everything here can be changed later from the machine's settings.
        </DialogDescription>
      </DialogHeader>

      <form
        id="create-machine"
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          create.mutate({ data: toRequest(draft) });
        }}
      >
        <Field id="name" label="Name">
          <Input
            id="name"
            required
            autoFocus
            value={draft.name}
            onChange={(event) => update({ name: event.target.value })}
          />
        </Field>

        <div className="grid grid-cols-2 gap-4">
          <Field id="guest-os" label="Operating system">
            <ChoiceSelect
              id="guest-os"
              value={draft.guestOs}
              choices={guestOsChoices}
              onChange={(guestOs) =>
                update({
                  guestOs,
                  ...defaultsFor(guestOs),
                  backend: supportsVz(guestOs) ? draft.backend : "qemu",
                })
              }
            />
          </Field>
          <Field id="arch" label="Architecture">
            <ChoiceSelect
              id="arch"
              disabled={vz}
              value={draft.arch}
              choices={archChoices(hostArch)}
              onChange={(arch) =>
                update({ arch, bios: arch === "x86_64" && draft.bios })
              }
            />
          </Field>
        </div>

        {vzAvailable && (
          <Field
            id="backend"
            label="Hypervisor"
            hint={
              vz
                ? "Text console only for now, and disks must be raw .img images."
                : undefined
            }
          >
            <ChoiceSelect
              id="backend"
              value={draft.backend}
              choices={[
                { value: "qemu", label: "QEMU (recommended)" },
                {
                  value: "vz",
                  label: "Apple Virtualization",
                  disabled: !supportsVz(draft.guestOs),
                },
              ]}
              onChange={(backend) => update({ backend })}
            />
          </Field>
        )}

        <div className="grid grid-cols-2 gap-4">
          <Field id="cores" label="CPU cores">
            <Input
              id="cores"
              type="number"
              min={1}
              max={64}
              required
              value={draft.cpuCores}
              onChange={(event) =>
                update({ cpuCores: Number(event.target.value) })
              }
            />
          </Field>
          <Field id="memory" label="Memory (GiB)">
            <Input
              id="memory"
              type="number"
              min={0.25}
              step={0.25}
              required
              value={draft.memoryGib}
              onChange={(event) =>
                update({ memoryGib: Number(event.target.value) })
              }
            />
          </Field>
        </div>

        <Field id="boot-disk" label="Boot disk">
          <ChoiceSelect
            id="boot-disk"
            value={draft.bootDisk}
            choices={[
              { value: "new", label: "New blank disk" },
              { value: "existing", label: "Existing image" },
              { value: "none", label: "None" },
            ]}
            onChange={(bootDisk) => update({ bootDisk })}
          />
        </Field>
        {draft.bootDisk === "new" && (
          <Field
            id="disk-size"
            label="Disk size (GiB)"
            hint="Created as a thin qcow2 image in the default image group."
          >
            <Input
              id="disk-size"
              type="number"
              min={1}
              required
              value={draft.newDiskGib}
              onChange={(event) =>
                update({ newDiskGib: Number(event.target.value) })
              }
            />
          </Field>
        )}
        {draft.bootDisk === "existing" && (
          <Field id="existing-disk" label="Image">
            <ChoiceSelect
              id="existing-disk"
              value={draft.existingDisk || NONE}
              choices={diskChoices}
              onChange={(value) =>
                update({ existingDisk: value === NONE ? "" : value })
              }
            />
          </Field>
        )}

        <Field
          id="install-media"
          label="Install media"
          hint="ISOs in an image group. Attached as a CD-ROM."
        >
          <ChoiceSelect
            id="install-media"
            value={draft.installMedia || NONE}
            choices={mediaChoices}
            onChange={(value) =>
              update({ installMedia: value === NONE ? "" : value })
            }
          />
        </Field>

        <Field id="console" label="Console">
          <ChoiceSelect
            id="console"
            disabled={vz}
            value={draft.console}
            choices={consoleChoices}
            onChange={(console) => update({ console })}
          />
        </Field>

        <div className="flex flex-col gap-2">
          {!vz && (
            <CheckboxField
              id="tpm"
              label="TPM 2.0 (needed by Windows 11)"
              checked={draft.tpm}
              onChange={(tpm) => update({ tpm })}
            />
          )}
          {!vz && draft.arch === "x86_64" && (
            <CheckboxField
              id="bios"
              label="Legacy BIOS instead of UEFI"
              checked={draft.bios}
              onChange={(bios) => update({ bios })}
            />
          )}
        </div>

        <Field id="notes" label="Notes">
          <Textarea
            id="notes"
            value={draft.notes}
            onChange={(event) => update({ notes: event.target.value })}
          />
        </Field>

        {create.isError && (
          <Alert variant="destructive">
            <AlertDescription>{create.error.message}</AlertDescription>
          </Alert>
        )}
      </form>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button
          type="submit"
          form="create-machine"
          variant="primary"
          disabled={create.isPending}
        >
          Create
        </Button>
      </DialogFooter>
    </DialogContent>
  );
}

function CheckboxField({
  id,
  label,
  checked,
  onChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <Checkbox
        id={id}
        checked={checked}
        onCheckedChange={(value) => onChange(value === true)}
      />
      <Label htmlFor={id}>{label}</Label>
    </div>
  );
}
