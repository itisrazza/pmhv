import { Button, Checkbox, Label } from "@swopstar/react-ui";
import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import type {
  Arch,
  DiskBus,
  DiskMedia,
  DiskSpec,
  ImageInfo,
} from "../../api/schemas";
import { busLabels, mediaLabels } from "../../lib/labels";
import { ChoiceSelect, type Choice } from "../form/ChoiceSelect";

/** QEMU's ARM UEFI firmware has no SATA driver, so it can't boot from SATA disks or CD-ROMs. */
function busChoices(arch: Arch): Choice<DiskBus>[] {
  return (Object.keys(busLabels) as DiskBus[]).map((bus) => ({
    value: bus,
    label:
      bus === "sata" && arch === "aarch64"
        ? `${busLabels[bus]} (not bootable on ARM)`
        : busLabels[bus],
    disabled: bus === "sata" && arch === "aarch64",
  }));
}

const mediaChoices: Choice<DiskMedia>[] = (
  Object.keys(mediaLabels) as DiskMedia[]
).map((media) => ({ value: media, label: mediaLabels[media] }));

const key = (disk: Pick<DiskSpec, "image">) =>
  `${disk.image.group}/${disk.image.name}`;

interface DiskListEditorProps {
  arch: Arch;
  disks: DiskSpec[];
  images: ImageInfo[];
  disabled: boolean;
  onChange: (disks: DiskSpec[]) => void;
}

/** Disks in boot order: the first one the firmware finds bootable wins. */
export function DiskListEditor({
  arch,
  disks,
  images,
  disabled,
  onChange,
}: DiskListEditorProps) {
  const imageChoices: Choice<string>[] = images.map((image) => ({
    value: `${image.group}/${image.name}`,
    label: image.name,
  }));
  const replace = (index: number, disk: DiskSpec) =>
    onChange(disks.map((existing, at) => (at === index ? disk : existing)));
  const move = (index: number, offset: number) => {
    const target = index + offset;
    if (target < 0 || target >= disks.length) return;
    const next = [...disks];
    [next[index], next[target]] = [next[target], next[index]];
    onChange(next);
  };
  const add = () => {
    const first = images[0];
    if (!first) return;
    onChange([
      ...disks,
      {
        image: { group: first.group, name: first.name },
        bus: "virtio",
        media: first.format === "iso" ? "cdrom" : "disk",
        read_only: false,
      },
    ]);
  };

  return (
    <div className="flex flex-col gap-3">
      {disks.length === 0 && (
        <p className="text-sm text-muted-foreground">No disks attached.</p>
      )}
      {disks.map((disk, index) => (
        <div
          key={`${index}-${key(disk)}`}
          className="flex flex-col gap-2 rounded-md border border-border p-3"
        >
          <div className="flex items-center gap-2">
            <span className="w-6 text-sm text-muted-foreground">
              {index + 1}
            </span>
            <div className="min-w-0 flex-1">
              <ChoiceSelect
                id={`disk-${index}-image`}
                value={key(disk)}
                choices={
                  imageChoices.some((choice) => choice.value === key(disk))
                    ? imageChoices
                    : [
                        ...imageChoices,
                        {
                          value: key(disk),
                          label: `${disk.image.name} (missing)`,
                        },
                      ]
                }
                onChange={(value) => {
                  const separator = value.indexOf("/");
                  replace(index, {
                    ...disk,
                    image: {
                      group: value.slice(0, separator),
                      name: value.slice(separator + 1),
                    },
                  });
                }}
              />
            </div>
            <Button
              size="icon-sm"
              title="Boot earlier"
              disabled={disabled || index === 0}
              onClick={() => move(index, -1)}
            >
              <ArrowUp />
            </Button>
            <Button
              size="icon-sm"
              title="Boot later"
              disabled={disabled || index === disks.length - 1}
              onClick={() => move(index, 1)}
            >
              <ArrowDown />
            </Button>
            <Button
              size="icon-sm"
              title="Detach (the image is kept)"
              disabled={disabled}
              onClick={() => onChange(disks.filter((_, at) => at !== index))}
            >
              <Trash2 />
            </Button>
          </div>
          <div className="flex flex-wrap items-center gap-3 pl-8">
            <div className="w-32">
              <ChoiceSelect
                id={`disk-${index}-bus`}
                value={disk.bus}
                choices={busChoices(arch)}
                onChange={(bus) => replace(index, { ...disk, bus })}
              />
            </div>
            <div className="w-32">
              <ChoiceSelect
                id={`disk-${index}-media`}
                value={disk.media}
                choices={mediaChoices}
                onChange={(media) => replace(index, { ...disk, media })}
              />
            </div>
            <div className="flex items-center gap-2">
              <Checkbox
                id={`disk-${index}-read-only`}
                checked={disk.read_only ?? false}
                onCheckedChange={(checked) =>
                  replace(index, { ...disk, read_only: checked === true })
                }
              />
              <Label htmlFor={`disk-${index}-read-only`}>Read-only</Label>
            </div>
          </div>
        </div>
      ))}
      <div>
        <Button
          size="sm"
          disabled={disabled || images.length === 0}
          onClick={add}
        >
          <Plus /> Attach image
        </Button>
      </div>
    </div>
  );
}
