import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@swopstar/react-ui";
import { RotateCcw, Zap } from "lucide-react";
import { useGetMachine, usePowerMachine } from "../api/machines/machines";
import type { Machine, PowerAction } from "../api/schemas";
import { StateBadge } from "../components/StateBadge";
import {
  ComboMenu,
  FullscreenButton,
  PanelToggles,
  ToolbarGroup,
  TypeClipboardButton,
} from "../components/console/ConsoleToolbar";
import {
  GraphicalConsole,
  type GraphicalSession,
} from "../components/console/GraphicalConsole";
import {
  TextConsole,
  type TextSession,
} from "../components/console/TextConsole";
import { VirtualKeyboard } from "../components/console/VirtualKeyboard";
import { VirtualMouse } from "../components/console/VirtualMouse";
import {
  enterFullscreen,
  keyboardLockSupported,
} from "../components/console/fullscreen";
import {
  graphicalCombos,
  keys,
  textCombos,
  type Key,
} from "../components/console/keys";
import type { ConnectionState } from "../components/console/socket-url";

const heldModifiers: Key[] = [keys.ctrl, keys.alt, keys.shift, keys.meta];

function PowerButtons({ machine }: { machine: Machine }) {
  const power = usePowerMachine({
    mutation: { onError: (error) => toast.error(error.message) },
  });
  const run = (action: PowerAction) =>
    power.mutate({ id: machine.id, data: { action } });
  return (
    <ToolbarGroup>
      <Button size="icon-sm" title="Reset" onClick={() => run("reset")}>
        <RotateCcw />
      </Button>
      <Button
        size="icon-sm"
        title="Force stop"
        onClick={() => run("force_stop")}
      >
        <Zap />
      </Button>
    </ToolbarGroup>
  );
}

function Disconnected({
  onReconnect,
  live,
}: {
  onReconnect: () => void;
  live: boolean;
}) {
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/70 text-white">
      <p>
        {live ? "Disconnected from the console." : "The machine isn't running."}
      </p>
      {live && <Button onClick={onReconnect}>Reconnect</Button>}
    </div>
  );
}

export function ConsolePage({ id }: { id: string }) {
  const query = useGetMachine(id);
  const machine = query.data;
  const root = useRef<HTMLDivElement>(null);
  const [attempt, setAttempt] = useState(0);
  const [connection, setConnection] = useState<ConnectionState>("connecting");
  const [text, setText] = useState<TextSession | null>(null);
  const [graphical, setGraphical] = useState<GraphicalSession | null>(null);
  const [held, setHeld] = useState<Set<number>>(new Set());
  const [showKeyboard, setShowKeyboard] = useState(false);
  const [showMouse, setShowMouse] = useState(false);

  const live =
    machine !== undefined &&
    ["running", "paused", "stopping"].includes(machine.state);
  const wasLive = useRef(live);
  useEffect(() => {
    if (live && !wasLive.current) {
      setAttempt((count) => count + 1);
    }
    wasLive.current = live;
  }, [live]);

  useEffect(() => {
    document.title = machine ? `${machine.spec.name} — console` : "Console";
  }, [machine]);

  const onTextSession = useCallback(
    (session: TextSession | null) => setText(session),
    [],
  );
  const onGraphicalSession = useCallback(
    (session: GraphicalSession | null) => setGraphical(session),
    [],
  );

  if (!machine) {
    return null;
  }

  const toggleModifier = (key: Key) => {
    const down = !held.has(key.keysym);
    graphical?.key(key, down);
    setHeld((current) => {
      const next = new Set(current);
      if (down) next.add(key.keysym);
      else next.delete(key.keysym);
      return next;
    });
  };

  const fullscreen = () => {
    if (!root.current) return;
    enterFullscreen(root.current)
      .then(() => graphical?.focus())
      .catch((error: unknown) =>
        toast.error(`Couldn't go fullscreen: ${String(error)}`),
      );
  };
  const fullscreenTitle = keyboardLockSupported()
    ? "Fullscreen, capturing system shortcuts (hold Esc to leave)"
    : "Fullscreen (this browser can't capture system shortcuts; use the combos menu)";

  const isGraphical = machine.spec.console === "graphical";

  return (
    <div ref={root} className="flex h-screen flex-col bg-black">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-background px-3 py-1.5 text-foreground">
        <div className="flex items-center gap-2">
          <span className="font-semibold">{machine.spec.name}</span>
          <StateBadge state={machine.state} />
        </div>
        <div className="flex flex-wrap items-center gap-3">
          {isGraphical ? (
            <>
              <ComboMenu
                label="Keys"
                combos={graphicalCombos}
                labelOf={(combo) => combo.label}
                onPick={(combo) => graphical?.combo(combo.keys)}
              />
              <ToolbarGroup>
                {heldModifiers.map((key) => (
                  <Button
                    key={key.code}
                    size="sm"
                    variant={held.has(key.keysym) ? "primary" : "default"}
                    title={`Hold ${key.label} for the next keys`}
                    onClick={() => toggleModifier(key)}
                  >
                    {key.label}
                  </Button>
                ))}
              </ToolbarGroup>
              <TypeClipboardButton onText={(value) => graphical?.type(value)} />
              <PanelToggles
                keyboard={showKeyboard}
                mouse={showMouse}
                onKeyboard={() => setShowKeyboard((shown) => !shown)}
                onMouse={() => setShowMouse((shown) => !shown)}
              />
            </>
          ) : (
            <>
              <ComboMenu
                label="Keys"
                combos={textCombos}
                labelOf={(combo) => combo.label}
                onPick={(combo) => text?.send(combo.bytes)}
              />
              <TypeClipboardButton onText={(value) => text?.send(value)} />
            </>
          )}
          <FullscreenButton onClick={fullscreen} title={fullscreenTitle} />
          <PowerButtons machine={machine} />
        </div>
      </div>

      <div className="relative min-h-0 flex-1">
        {live &&
          (isGraphical ? (
            <GraphicalConsole
              machineId={machine.id}
              attempt={attempt}
              onState={setConnection}
              onSession={onGraphicalSession}
            />
          ) : (
            <TextConsole
              machineId={machine.id}
              attempt={attempt}
              onState={setConnection}
              onSession={onTextSession}
            />
          ))}
        {(!live || connection === "disconnected") && (
          <Disconnected
            live={live}
            onReconnect={() => setAttempt((count) => count + 1)}
          />
        )}
      </div>

      {isGraphical && showMouse && graphical && (
        <VirtualMouse canvas={graphical.canvas} />
      )}
      {isGraphical && showKeyboard && (
        <VirtualKeyboard
          held={held}
          onToggleModifier={toggleModifier}
          onPress={(key) => graphical?.combo([key])}
        />
      )}
    </div>
  );
}
