import { FileBox } from "lucide-react";
import { EmptyState } from "../components/EmptyState";

export function DiskImagesPage() {
  return (
    <EmptyState
      icon={FileBox}
      title="No disk images yet"
      description="Disk images and ISOs will be listed here once the image store is in place."
    />
  );
}
