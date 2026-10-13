import { useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@swopstar/react-ui";
import {
  Menu,
  Pause,
  Play,
  Power,
  RotateCcw,
  Settings,
  Trash2,
  Zap,
} from "lucide-react";
import { useDeleteMachine, usePowerMachine } from "../api/machines/machines";
import type { Machine, PowerAction } from "../api/schemas";

function useActions(machine: Machine) {
  const power = usePowerMachine({
    mutation: { onError: (error) => toast.error(error.message) },
  });
  return {
    pending: power.isPending,
    run: (action: PowerAction) =>
      power.mutate({ id: machine.id, data: { action } }),
  };
}

export function PowerControls({ machine }: { machine: Machine }) {
  const { pending, run } = useActions(machine);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const stopped = machine.state === "stopped";
  const live = !stopped && machine.state !== "starting";
  const paused = machine.state === "paused";

  return (
    <div className="flex items-center gap-2">
      <Button
        size="icon"
        variant={stopped ? "primary" : "default"}
        disabled={pending || machine.state === "starting"}
        title={stopped ? "Start" : "Shut down"}
        onClick={() => run(stopped ? "start" : "stop")}
      >
        <Power />
      </Button>
      <Button
        size="icon"
        disabled={pending || !live || machine.state === "stopping"}
        title={paused ? "Resume" : "Pause"}
        onClick={() => run(paused ? "resume" : "pause")}
      >
        {paused ? <Play /> : <Pause />}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button size="icon" title="More actions">
            <Menu />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem disabled={!live} onClick={() => run("reset")}>
            <RotateCcw /> Reset
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={stopped}
            onClick={() => run("force_stop")}
          >
            <Zap /> Force stop
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            disabled={!stopped}
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 /> Delete…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Button size="icon" title="Settings" asChild>
        <Link to="/machines/$id/settings" params={{ id: machine.id }}>
          <Settings />
        </Link>
      </Button>
      <DeleteDialog
        machine={machine}
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
      />
    </div>
  );
}

function DeleteDialog({
  machine,
  open,
  onOpenChange,
}: {
  machine: Machine;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const remove = useDeleteMachine({
    mutation: {
      onSuccess: () => void navigate({ to: "/machines" }),
      onError: (error) => toast.error(error.message),
    },
  });
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete {machine.spec.name}?</AlertDialogTitle>
          <AlertDialogDescription>
            This removes the machine's settings, firmware variables and TPM
            state. Its disk images stay in Disk images, so delete those there if
            you don't need them.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() => remove.mutate({ id: machine.id })}
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
