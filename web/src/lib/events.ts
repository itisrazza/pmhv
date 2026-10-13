import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import {
  getGetMachineQueryKey,
  getListMachinesQueryKey,
} from "../api/machines/machines";
import type { Event, Machine } from "../api/schemas";

const EVENTS_URL = "/api/v1/events";

function upsert(machines: Machine[], machine: Machine): Machine[] {
  const others = machines.filter((existing) => existing.id !== machine.id);
  return [...others, machine].sort((a, b) =>
    a.spec.name.localeCompare(b.spec.name, undefined, { sensitivity: "base" }),
  );
}

function apply(queryClient: QueryClient, event: Event) {
  switch (event.type) {
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
