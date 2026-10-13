import type { ReactNode } from "react";
import { toast } from "sonner";
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@swopstar/react-ui";
import {
  ChevronDown,
  ClipboardType,
  Keyboard,
  Maximize,
  Mouse,
} from "lucide-react";

export function ToolbarGroup({ children }: { children: ReactNode }) {
  return <div className="flex items-center gap-1">{children}</div>;
}

export function ComboMenu<T>({
  label,
  combos,
  labelOf,
  onPick,
}: {
  label: string;
  combos: readonly T[];
  labelOf: (combo: T) => string;
  onPick: (combo: T) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm">
          {label} <ChevronDown />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        {combos.map((combo) => (
          <DropdownMenuItem key={labelOf(combo)} onClick={() => onPick(combo)}>
            {labelOf(combo)}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Sends the clipboard as keystrokes; guests rarely have clipboard integration, least of all at boot. */
export function TypeClipboardButton({
  onText,
}: {
  onText: (text: string) => void;
}) {
  return (
    <Button
      size="sm"
      title="Type the clipboard's text into the guest"
      onClick={() => {
        navigator.clipboard
          .readText()
          .then(onText)
          .catch((error: unknown) =>
            toast.error(`Couldn't read the clipboard: ${String(error)}`),
          );
      }}
    >
      <ClipboardType /> Type clipboard
    </Button>
  );
}

export function PanelToggles({
  keyboard,
  mouse,
  onKeyboard,
  onMouse,
}: {
  keyboard: boolean;
  mouse?: boolean;
  onKeyboard: () => void;
  onMouse?: () => void;
}) {
  return (
    <ToolbarGroup>
      <Button
        size="icon-sm"
        variant={keyboard ? "primary" : "default"}
        title="Virtual keyboard"
        onClick={onKeyboard}
      >
        <Keyboard />
      </Button>
      {onMouse && (
        <Button
          size="icon-sm"
          variant={mouse ? "primary" : "default"}
          title="Virtual mouse"
          onClick={onMouse}
        >
          <Mouse />
        </Button>
      )}
    </ToolbarGroup>
  );
}

export function FullscreenButton({
  onClick,
  title,
}: {
  onClick: () => void;
  title: string;
}) {
  return (
    <Button size="icon-sm" title={title} onClick={onClick}>
      <Maximize />
    </Button>
  );
}
