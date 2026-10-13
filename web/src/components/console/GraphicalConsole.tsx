import { useEffect, useRef } from "react";
import RFB from "@novnc/novnc";
import { consoleSocketUrl, type ConnectionState } from "./socket-url";
import type { Key } from "./keys";

export interface GraphicalSession {
  /** Presses keys in order, then releases them in reverse, like a person typing a combo. */
  combo: (keys: Key[]) => void;
  key: (key: Key, down: boolean) => void;
  type: (text: string) => void;
  /** The canvas noVNC draws into, for synthesising pointer input. */
  canvas: () => HTMLCanvasElement | null;
  focus: () => void;
}

interface GraphicalConsoleProps {
  machineId: string;
  attempt: number;
  onState: (state: ConnectionState) => void;
  onSession: (session: GraphicalSession | null) => void;
}

/** For printable ASCII and Latin-1, the X11 keysym is the code point. */
function keysymFor(character: string): number {
  const code = character.codePointAt(0) ?? 0;
  if (character === "\n") return 0xff0d;
  if (character === "\t") return 0xff09;
  return code < 0x100 ? code : 0x01000000 + code;
}

export function GraphicalConsole({
  machineId,
  attempt,
  onState,
  onSession,
}: GraphicalConsoleProps) {
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = container.current;
    if (!element) {
      return;
    }
    onState("connecting");
    const rfb = new RFB(element, consoleSocketUrl(machineId, "vnc"), {
      wsProtocols: ["binary"],
    });
    rfb.scaleViewport = true;
    rfb.background = "#000";

    const session: GraphicalSession = {
      combo: (keys) => {
        keys.forEach((key) => rfb.sendKey(key.keysym, key.code, true));
        [...keys]
          .reverse()
          .forEach((key) => rfb.sendKey(key.keysym, key.code, false));
        rfb.focus();
      },
      key: (key, down) => rfb.sendKey(key.keysym, key.code, down),
      type: (text) => {
        for (const character of text) {
          rfb.sendKey(keysymFor(character), null);
        }
        rfb.focus();
      },
      canvas: () => element.querySelector("canvas"),
      focus: () => rfb.focus(),
    };
    rfb.addEventListener("connect", () => {
      onState("connected");
      onSession(session);
      rfb.focus();
    });
    rfb.addEventListener("disconnect", () => {
      onState("disconnected");
      onSession(null);
    });

    return () => {
      onSession(null);
      rfb.disconnect();
    };
  }, [machineId, attempt, onState, onSession]);

  return <div ref={container} className="h-full w-full bg-black" />;
}
