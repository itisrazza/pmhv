export function consoleSocketUrl(
  machineId: string,
  kind: "serial" | "vnc",
): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/api/v1/machines/${machineId}/console/${kind}`;
}

export type ConnectionState = "connecting" | "connected" | "disconnected";
