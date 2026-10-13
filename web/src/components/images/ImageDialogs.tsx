import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
} from "@swopstar/react-ui";
import {
  getListImagesQueryKey,
  useCreateImage,
  useDeleteImage,
  useResizeImage,
} from "../../api/images/images";
import type { ImageInfo } from "../../api/schemas";
import { ChoiceSelect } from "../form/ChoiceSelect";
import { Field } from "../form/Field";

const GIB = 1024 ** 3;

function useRefreshImages() {
  const queryClient = useQueryClient();
  return () =>
    queryClient.invalidateQueries({ queryKey: getListImagesQueryKey() });
}

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function NewImageDialog({
  open,
  onOpenChange,
  groups,
}: DialogProps & { groups: string[] }) {
  const [name, setName] = useState("");
  const [group, setGroup] = useState(groups[0] ?? "default");
  const [sizeGib, setSizeGib] = useState(32);
  const refresh = useRefreshImages();
  const create = useCreateImage({
    mutation: {
      onSuccess: () => {
        void refresh();
        onOpenChange(false);
        setName("");
      },
      onError: (error) => toast.error(error.message),
    },
  });
  const trimmed = name.trim();
  const fileName = trimmed.endsWith(".qcow2") ? trimmed : `${trimmed}.qcow2`;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>New blank disk image</DialogTitle>
        </DialogHeader>
        <form
          id="new-image"
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            create.mutate({
              data: { group, name: fileName, size_gib: sizeGib },
            });
          }}
        >
          <Field
            id="image-name"
            label="Name"
            hint="Created as a thin qcow2 image, so it only uses space as the guest writes to it."
          >
            <Input
              id="image-name"
              required
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </Field>
          {groups.length > 1 && (
            <Field id="image-group" label="Image group">
              <ChoiceSelect
                id="image-group"
                value={group}
                choices={groups.map((value) => ({ value, label: value }))}
                onChange={setGroup}
              />
            </Field>
          )}
          <Field id="image-size" label="Size (GiB)">
            <Input
              id="image-size"
              type="number"
              min={1}
              required
              value={sizeGib}
              onChange={(event) => setSizeGib(Number(event.target.value))}
            />
          </Field>
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="new-image"
            variant="primary"
            disabled={create.isPending}
          >
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function ResizeImageDialog({
  open,
  onOpenChange,
  image,
}: DialogProps & { image: ImageInfo }) {
  const currentGib = Math.ceil(image.virtual_bytes / GIB);
  const [sizeGib, setSizeGib] = useState(currentGib);
  const refresh = useRefreshImages();
  const resize = useResizeImage({
    mutation: {
      onSuccess: () => {
        void refresh();
        onOpenChange(false);
      },
      onError: (error) => toast.error(error.message),
    },
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Resize {image.name}</DialogTitle>
        </DialogHeader>
        <form
          id="resize-image"
          onSubmit={(event) => {
            event.preventDefault();
            resize.mutate({
              group: image.group,
              name: image.name,
              data: { size_gib: sizeGib },
            });
          }}
        >
          <Field
            id="resize-size"
            label="New size (GiB)"
            hint="Images can only grow. Afterwards, grow the partition and file system inside the guest."
          >
            <Input
              id="resize-size"
              type="number"
              min={currentGib}
              required
              value={sizeGib}
              onChange={(event) => setSizeGib(Number(event.target.value))}
            />
          </Field>
        </form>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="resize-image"
            variant="primary"
            disabled={resize.isPending}
          >
            Resize
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function DeleteImageDialog({
  open,
  onOpenChange,
  image,
}: DialogProps & { image: ImageInfo }) {
  const refresh = useRefreshImages();
  const remove = useDeleteImage({
    mutation: {
      onSuccess: () => void refresh(),
      onError: (error) => toast.error(error.message),
    },
  });
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete {image.name}?</AlertDialogTitle>
          <AlertDialogDescription>
            The file is deleted from the host. This can't be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() =>
              remove.mutate({ group: image.group, name: image.name })
            }
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
