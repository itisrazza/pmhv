import { useRef, useState, type PointerEvent } from "react";
import { Button } from "@swopstar/react-ui";
import { ChevronDown, ChevronUp } from "lucide-react";

interface VirtualMouseProps {
  canvas: () => HTMLCanvasElement | null;
}

type MouseButton = 0 | 1 | 2;

/**
 * Drives the guest pointer by synthesising events on noVNC's canvas, for when the real pointer
 * is stuck or grabbed. The trackpad moves a virtual position; buttons click there.
 */
export function VirtualMouse({ canvas }: VirtualMouseProps) {
  const position = useRef({ x: 0.5, y: 0.5 });
  const drag = useRef<{ x: number; y: number } | null>(null);
  const [shown, setShown] = useState({ x: 0.5, y: 0.5 });

  const dispatch = (type: string, button: MouseButton = 0, buttons = 0) => {
    const target = canvas();
    if (!target) {
      return;
    }
    const bounds = target.getBoundingClientRect();
    const init = {
      bubbles: true,
      clientX: bounds.left + position.current.x * bounds.width,
      clientY: bounds.top + position.current.y * bounds.height,
      button,
      buttons,
    };
    target.dispatchEvent(new MouseEvent(type, init));
  };

  const click = (button: MouseButton) => {
    const mask = [1, 4, 2][button];
    dispatch("mousedown", button, mask);
    dispatch("mouseup", button, 0);
  };

  const scroll = (deltaY: number) => {
    const target = canvas();
    if (!target) {
      return;
    }
    const bounds = target.getBoundingClientRect();
    target.dispatchEvent(
      new WheelEvent("wheel", {
        bubbles: true,
        clientX: bounds.left + position.current.x * bounds.width,
        clientY: bounds.top + position.current.y * bounds.height,
        deltaY,
      }),
    );
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) {
      return;
    }
    const pad = event.currentTarget.getBoundingClientRect();
    const dx = (event.clientX - drag.current.x) / pad.width;
    const dy = (event.clientY - drag.current.y) / pad.height;
    drag.current = { x: event.clientX, y: event.clientY };
    position.current = {
      x: Math.min(1, Math.max(0, position.current.x + dx)),
      y: Math.min(1, Math.max(0, position.current.y + dy)),
    };
    setShown(position.current);
    dispatch("mousemove");
  };

  return (
    <div className="flex items-center gap-2 border-t border-border bg-background p-2">
      <div
        className="relative h-28 w-44 touch-none rounded-md border border-border bg-muted"
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId);
          drag.current = { x: event.clientX, y: event.clientY };
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerMove={onPointerMove}
      >
        <span
          className="absolute size-2 -translate-1/2 rounded-full bg-primary"
          style={{ left: `${shown.x * 100}%`, top: `${shown.y * 100}%` }}
        />
      </div>
      <div className="flex flex-col gap-1">
        <div className="flex gap-1">
          <Button size="xs" onClick={() => click(0)}>
            Left
          </Button>
          <Button size="xs" onClick={() => click(1)}>
            Middle
          </Button>
          <Button size="xs" onClick={() => click(2)}>
            Right
          </Button>
        </div>
        <div className="flex gap-1">
          <Button size="xs" title="Scroll up" onClick={() => scroll(-120)}>
            <ChevronUp />
          </Button>
          <Button size="xs" title="Scroll down" onClick={() => scroll(120)}>
            <ChevronDown />
          </Button>
        </div>
      </div>
    </div>
  );
}
