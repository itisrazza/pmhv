import { Network } from "lucide-react";
import { EmptyState } from "../components/EmptyState";

export function NetworksPage() {
  return (
    <EmptyState
      icon={Network}
      title="Built-in NAT only"
      description="Machines use their backend's built-in NAT for now. Isolated networks come after the MVP."
    />
  );
}
