import { useQuery } from "@tanstack/react-query";
import { Monitor, SquareTerminal } from "lucide-react";
import type { Machine } from "../api/schemas";

function thumbnailUrl(machine: Machine): string {
  // The capture time busts the browser cache whenever there's a new capture.
  return `/api/v1/machines/${machine.id}/thumbnail?at=${machine.thumbnail?.captured_at ?? 0}`;
}

function TextThumbnail({ machine }: { machine: Machine }) {
  const url = thumbnailUrl(machine);
  const text = useQuery({
    queryKey: ["thumbnail", url],
    queryFn: async () => {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`thumbnail request failed: ${response.status}`);
      }
      return response.text();
    },
    staleTime: Infinity,
  });
  return (
    <pre className="flex h-full w-full flex-col justify-end overflow-hidden p-2 font-mono text-[7px] leading-tight whitespace-pre text-white/80">
      {text.data}
    </pre>
  );
}

/** The machine's screen as last captured, or a placeholder when there's nothing to show. */
export function Thumbnail({ machine }: { machine: Machine }) {
  const Placeholder =
    machine.spec.console === "graphical" ? Monitor : SquareTerminal;
  return (
    <div className="flex aspect-video w-full items-center justify-center overflow-hidden rounded-md bg-black">
      {machine.thumbnail?.kind === "image" && (
        <img
          src={thumbnailUrl(machine)}
          alt=""
          className="h-full w-full object-contain"
        />
      )}
      {machine.thumbnail?.kind === "text" && (
        <TextThumbnail machine={machine} />
      )}
      {!machine.thumbnail && (
        <Placeholder size={32} className="text-white/30" />
      )}
    </div>
  );
}
