import { Network } from "lucide-react";
import { EmptyState } from "../components/EmptyState";
import { PageTitle } from "../components/PageHeaderSlot";

export function NetworksPage() {
  return (
    <>
      <PageTitle title="Networks" />
      <EmptyState
        icon={Network}
        title="Built-in NAT only"
        description="Machines use their backend's built-in NAT for now. Isolated networks come after the MVP."
      />
    </>
  );
}
