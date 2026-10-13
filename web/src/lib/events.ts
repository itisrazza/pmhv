import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import {
  getGetMachineQueryKey,
  getListMachinesQueryKey,
} from "../api/machines/machines";
import { toast } from "sonner";
import { getListImagesQueryKey } from "../api/images/images";
import type { Event, Job, Machine } from "../api/schemas";

const EVENTS_URL = "/api/v1/events";

function upsert(machines: Machine[], machine: Machine): Machine[] {
  const others = machines.filter((existing) => existing.id !== machine.id);
  return [...others, machine].sort((a, b) =>
    a.spec.name.localeCompare(b.spec.name, undefined, { sensitivity: "base" }),
  );
}

/** One toast per job, updated in place as it progresses. */
function showJob(queryClient: QueryClient, job: Job) {
  switch (job.state) {
    case "running":
      toast.loading(`${job.description}… ${Math.round(job.progress * 100)}%`, {
        id: job.id,
      });
      break;
    case "succeeded":
      toast.success(`${job.description}: done`, { id: job.id });
      void queryClient.invalidateQueries({ queryKey: getListImagesQueryKey() });
      break;
    case "failed":
      toast.error(
        `${job.description} failed: ${job.error ?? "unknown error"}`,
        {
          id: job.id,
        },
      );
      break;
  }
}

function apply(queryClient: QueryClient, event: Event) {
  switch (event.type) {
    case "job_updated":
      showJob(queryClient, event.job);
      break;
    case "machine_updated":
      queryClient.setQueryData<Machine[]>(
        getListMachinesQueryKey(),
        (current) => current && upsert(current, event.machine),
      );
      queryClient.setQueryData<Machine>(
        getGetMachineQueryKey(event.machine.id),
        event.machine,
      );
      break;
    case "machine_removed":
      queryClient.setQueryData<Machine[]>(
        getListMachinesQueryKey(),
        (current) => current?.filter((machine) => machine.id !== event.id),
      );
      queryClient.removeQueries({ queryKey: getGetMachineQueryKey(event.id) });
      break;
  }
}

/** Keeps machine queries current from pmhvd's event stream. */
export function useMachineEvents() {
  const queryClient = useQueryClient();

  useEffect(() => {
    const source = new EventSource(EVENTS_URL);
    // Anything missed while (re)connecting or lagging is recovered by refetching.
    const refetch = () => queryClient.invalidateQueries();
    source.addEventListener("open", refetch);
    source.addEventListener("lagged", refetch);
    source.addEventListener("message", (message: MessageEvent<string>) => {
      apply(queryClient, JSON.parse(message.data) as Event);
    });
    return () => source.close();
  }, [queryClient]);
}
