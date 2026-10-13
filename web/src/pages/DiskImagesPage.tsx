import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  Alert,
  AlertDescription,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@swopstar/react-ui";
import {
  Disc3,
  Download,
  FileBox,
  HardDrive,
  Menu,
  Plus,
  Scaling,
  Trash2,
  Upload,
} from "lucide-react";
import { getListImagesQueryKey, useListImages } from "../api/images/images";
import type { ImageGroup, ImageInfo } from "../api/schemas";
import { EmptyState } from "../components/EmptyState";
import { PageTitle } from "../components/PageHeaderSlot";
import {
  DeleteImageDialog,
  NewImageDialog,
  ResizeImageDialog,
} from "../components/images/ImageDialogs";
import { formatBytes } from "../lib/labels";
import { uploadImage } from "../lib/upload";

function sizeLabel(image: ImageInfo): string {
  if (image.format === "qcow2") {
    return `${formatBytes(image.allocated_bytes)} / ${formatBytes(image.virtual_bytes)}`;
  }
  return formatBytes(image.virtual_bytes);
}

function downloadUrl(image: ImageInfo): string {
  return `/api/v1/images/${encodeURIComponent(image.group)}/${encodeURIComponent(image.name)}`;
}

function ImageRow({ image }: { image: ImageInfo }) {
  const [dialog, setDialog] = useState<"resize" | "delete" | null>(null);
  const iso = image.format === "iso";
  const Icon = iso ? Disc3 : HardDrive;
  const inUse = image.used_by.length > 0;
  const users = image.used_by.map((user) => user.name).join(", ");

  return (
    <li className="flex items-center gap-3 border-b border-border px-4 py-3 last:border-b-0">
      <Icon size={22} className="shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate">{image.name}</span>
        <span className="text-sm text-muted-foreground">
          {iso ? "ISO image" : "Hard disk image"} · {sizeLabel(image)}
          {inUse && ` · used by ${users}`}
        </span>
      </div>
      <div className="flex items-center gap-2">
        <Button size="icon" title="Download" asChild>
          <a href={downloadUrl(image)} download={image.name}>
            <Download />
          </a>
        </Button>
        <Button
          size="icon"
          title={inUse ? `Used by ${users}` : "Delete"}
          disabled={inUse}
          onClick={() => setDialog("delete")}
        >
          <Trash2 />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon" title="More actions">
              <Menu />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              disabled={iso}
              onClick={() => setDialog("resize")}
            >
              <Scaling /> Resize…
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {dialog === "resize" && (
        <ResizeImageDialog
          open
          onOpenChange={(open) => !open && setDialog(null)}
          image={image}
        />
      )}
      <DeleteImageDialog
        open={dialog === "delete"}
        onOpenChange={(open) => !open && setDialog(null)}
        image={image}
      />
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

function useUploads(group: string) {
  const queryClient = useQueryClient();
  return (files: FileList) => {
    for (const file of Array.from(files)) {
      const toastId = toast.loading(`Uploading ${file.name}…`);
      uploadImage(group, file, (fraction) =>
        toast.loading(
          `Uploading ${file.name}… ${Math.round(fraction * 100)}%`,
          { id: toastId },
        ),
      )
        .then(() => {
          toast.success(`Uploaded ${file.name}`, { id: toastId });
          void queryClient.invalidateQueries({
            queryKey: getListImagesQueryKey(),
          });
        })
        .catch((error: unknown) =>
          toast.error(
            `Uploading ${file.name} failed: ${error instanceof Error ? error.message : String(error)}`,
            { id: toastId },
          ),
        );
    }
  };
}

export function DiskImagesPage() {
  const images = useListImages();
  const [creating, setCreating] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const groups = images.data?.map((group) => group.name) ?? [];
  const upload = useUploads(groups[0] ?? "default");
  const empty = images.data?.every((group) => group.images.length === 0);

  return (
    <>
      <PageTitle
        title="Disk images"
        actions={
          <>
            <Button
              size="icon"
              title="Upload to the default image group"
              onClick={() => fileInput.current?.click()}
            >
              <Upload />
            </Button>
            <Button
              size="icon"
              title="New blank disk image"
              onClick={() => setCreating(true)}
            >
              <Plus />
            </Button>
          </>
        }
      />
      <input
        ref={fileInput}
        type="file"
        multiple
        accept=".qcow2,.img,.raw,.iso"
        className="hidden"
        onChange={(event) => {
          if (event.target.files) {
            upload(event.target.files);
          }
          event.target.value = "";
        }}
      />
      <NewImageDialog
        open={creating}
        onOpenChange={setCreating}
        groups={groups}
      />
      {images.isError && (
        <Alert variant="destructive" className="m-6 w-auto">
          <AlertDescription>{images.error.message}</AlertDescription>
        </Alert>
      )}
      {empty && (
        <EmptyState
          icon={FileBox}
          title="No disk images yet"
          description="Upload ISOs or disk images, or create a blank one. New machines' disks appear here too."
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
