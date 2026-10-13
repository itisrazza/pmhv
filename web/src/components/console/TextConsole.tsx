import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { consoleSocketUrl, type ConnectionState } from "./socket-url";

export interface TextSession {
  send: (text: string) => void;
}

interface TextConsoleProps {
  machineId: string;
  /** Changes to reconnect. */
  attempt: number;
  onState: (state: ConnectionState) => void;
  onSession: (session: TextSession | null) => void;
}

export function TextConsole({
  machineId,
  attempt,
  onState,
  onSession,
}: TextConsoleProps) {
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = container.current;
    if (!element) {
      return;
    }
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: "ui-monospace, Menlo, monospace",
      scrollback: 10_000,
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(element);
    fit.fit();
    const resize = new ResizeObserver(() => fit.fit());
    resize.observe(element);

    onState("connecting");
    const socket = new WebSocket(consoleSocketUrl(machineId, "serial"));
    socket.binaryType = "arraybuffer";
    const encoder = new TextEncoder();
    const send = (text: string) => {
      if (socket.readyState === WebSocket.OPEN) {
        socket.send(encoder.encode(text));
      }
    };
    socket.addEventListener("open", () => {
      onState("connected");
      onSession({ send });
      terminal.focus();
    });
    socket.addEventListener("close", () => {
      onState("disconnected");
      onSession(null);
    });
    socket.addEventListener("message", (message: MessageEvent<ArrayBuffer>) => {
      terminal.write(new Uint8Array(message.data));
    });
    const input = terminal.onData(send);

    return () => {
      input.dispose();
      socket.close();
      resize.disconnect();
      terminal.dispose();
    };
  }, [machineId, attempt, onState, onSession]);

  return <div ref={container} className="h-full w-full bg-black p-2" />;
}
