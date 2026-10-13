import { Box } from "lucide-react";
import { EmptyState } from "../components/EmptyState";

export function MachinesPage() {
  return (
    <EmptyState
      icon={Box}
      title="No machines yet"
      description="Creating and running machines arrives with the QEMU backend."
    />
  );
}
