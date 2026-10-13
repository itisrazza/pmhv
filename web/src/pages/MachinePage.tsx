import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
} from "@swopstar/react-ui";
import { ExternalLink } from "lucide-react";
import { useGetMachine } from "../api/machines/machines";
import type { Machine } from "../api/schemas";
import { HardwareList } from "../components/HardwareList";
import { PageTitle } from "../components/PageHeaderSlot";
import { PowerControls } from "../components/PowerControls";
import { StateBadge } from "../components/StateBadge";
import { Thumbnail } from "../components/Thumbnail";

/** Consoles always open in their own tab; one tab per machine, reused if already open. */
export function openConsole(machine: Machine) {
  window.open(`/machines/${machine.id}/console`, `pmhv-console-${machine.id}`);
}

function ConsolePreview({ machine }: { machine: Machine }) {
  const live = ["running", "paused", "stopping"].includes(machine.state);
  return (
    <div className="relative">
      <Thumbnail machine={machine} />
      <StateBadge state={machine.state} className="absolute top-3 left-3" />
      <div className="absolute inset-0 flex items-center justify-center">
        <Button disabled={!live} onClick={() => openConsole(machine)}>
          <ExternalLink /> Open console
        </Button>
      </div>
    </div>
  );
}

export function MachinePage({ id }: { id: string }) {
  const query = useGetMachine(id);

  if (query.isError) {
    return (
      <Alert variant="destructive" className="m-6 w-auto">
        <AlertTitle>Couldn't load this machine</AlertTitle>
        <AlertDescription>{query.error.message}</AlertDescription>
      </Alert>
    );
  }
  const machine = query.data;
  if (!machine) {
    return null;
  }

  return (
    <>
      <PageTitle
        title={machine.spec.name}
        actions={<PowerControls machine={machine} />}
      />
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-6 p-6">
        <ConsolePreview machine={machine} />
        {machine.last_error && (
          <Alert variant="destructive">
            <AlertTitle>The machine stopped with an error</AlertTitle>
            <AlertDescription className="break-words">
              {machine.last_error}
            </AlertDescription>
          </Alert>
        )}
        <HardwareList machine={machine} />
        {machine.spec.notes && (
          <section className="flex flex-col gap-2">
            <h2 className="font-semibold">Notes</h2>
            <p className="rounded-md border border-border bg-card p-3 whitespace-pre-wrap">
              {machine.spec.notes}
            </p>
          </section>
        )}
      </div>
    </>
  );
}
