import { useState } from "react";
import { Alert, AlertDescription, Button } from "@swopstar/react-ui";
import { Box, Plus } from "lucide-react";
import { useListMachines } from "../api/machines/machines";
import { CreateMachineDialog } from "../components/CreateMachineDialog";
import { EmptyState } from "../components/EmptyState";
import { MachineCard } from "../components/MachineCard";
import { PageTitle } from "../components/PageHeaderSlot";

export function MachinesPage() {
  const machines = useListMachines();
  const [creating, setCreating] = useState(false);

  return (
    <>
      <PageTitle
        title="Machines"
        actions={
          <Button
            size="icon"
            title="New machine"
            onClick={() => setCreating(true)}
          >
            <Plus />
          </Button>
        }
      />
      <CreateMachineDialog open={creating} onOpenChange={setCreating} />
      {machines.isError && (
        <Alert variant="destructive" className="m-6 w-auto">
          <AlertDescription>{machines.error.message}</AlertDescription>
        </Alert>
      )}
      {machines.data?.length === 0 && (
        <EmptyState
          icon={Box}
          title="No machines yet"
          description="Create one with the + button."
        />
      )}
      {machines.data && machines.data.length > 0 && (
        <div className="grid gap-3 p-6 [grid-template-columns:repeat(auto-fill,minmax(260px,1fr))]">
          {machines.data.map((machine) => (
            <MachineCard key={machine.id} machine={machine} />
          ))}
        </div>
      )}
    </>
  );
}
