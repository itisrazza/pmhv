import { Alert, AlertDescription } from "@swopstar/react-ui";
import { Disc3, FileBox, HardDrive } from "lucide-react";
import { useListImages } from "../api/images/images";
import type { ImageGroup, ImageInfo } from "../api/schemas";
import { EmptyState } from "../components/EmptyState";
import { PageTitle } from "../components/PageHeaderSlot";
import { formatBytes } from "../lib/labels";

function sizeLabel(image: ImageInfo): string {
  if (image.format === "qcow2") {
    return `${formatBytes(image.allocated_bytes)} / ${formatBytes(image.virtual_bytes)}`;
  }
  return formatBytes(image.virtual_bytes);
}

function ImageRow({ image }: { image: ImageInfo }) {
  const iso = image.format === "iso";
  const Icon = iso ? Disc3 : HardDrive;
  return (
    <li className="flex items-center gap-3 border-b border-border px-4 py-3 last:border-b-0">
      <Icon size={22} className="shrink-0" />
      <div className="flex min-w-0 flex-col">
        <span className="truncate">{image.name}</span>
        <span className="text-sm text-muted-foreground">
          {iso ? "ISO image" : "Hard disk image"} · {sizeLabel(image)}
        </span>
      </div>
    </li>
  );
}

function Group({ group }: { group: ImageGroup }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="font-semibold">
        {group.name === "default" ? "Default image group" : group.name}
      </h2>
      {group.images.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No images in this group.
        </p>
      ) : (
        <ul className="rounded-lg border border-border bg-card">
          {group.images.map((image) => (
            <ImageRow key={image.name} image={image} />
          ))}
        </ul>
      )}
    </section>
  );
}

export function DiskImagesPage() {
  const images = useListImages();
  const empty = images.data?.every((group) => group.images.length === 0);

  return (
    <>
      <PageTitle title="Disk images" />
      {images.isError && (
        <Alert variant="destructive" className="m-6 w-auto">
          <AlertDescription>{images.error.message}</AlertDescription>
        </Alert>
      )}
      {empty && (
        <EmptyState
          icon={FileBox}
          title="No disk images yet"
          description="Machines' new disks appear here. Until uploads arrive, copy ISOs into the image group's folder on the host."
        />
      )}
      {images.data && !empty && (
        <div className="mx-auto flex w-full max-w-4xl flex-col gap-6 p-6">
          {images.data.map((group) => (
            <Group key={group.name} group={group} />
          ))}
        </div>
      )}
    </>
  );
}
