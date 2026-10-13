import { Button, cn } from "@swopstar/react-ui";
import { functionKey, isModifier, keys, letterKey, type Key } from "./keys";

function characterKey(character: string, code: string): Key {
  return { label: character, keysym: character.charCodeAt(0), code };
}

const digitRow: Key[] = [..."1234567890"].map((digit) =>
  characterKey(digit, `Digit${digit}`),
);
const letterRows: Key[][] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"].map((row) =>
  [...row].map(letterKey),
);

const rows: Key[][] = [
  [
    keys.escape,
    ...Array.from({ length: 12 }, (_, index) => functionKey(index + 1)),
  ],
  [...digitRow, keys.backspace],
  [keys.tab, ...letterRows[0]],
  [...letterRows[1], keys.enter],
  [keys.shift, ...letterRows[2], keys.up],
  [
    keys.ctrl,
    keys.meta,
    keys.alt,
    keys.space,
    keys.left,
    keys.down,
    keys.right,
    keys.delete,
  ],
];

interface VirtualKeyboardProps {
  held: Set<number>;
  onToggleModifier: (key: Key) => void;
  onPress: (key: Key) => void;
}

/** For when the physical keyboard path is broken or the layout's wrong. */
export function VirtualKeyboard({
  held,
  onToggleModifier,
  onPress,
}: VirtualKeyboardProps) {
  return (
    <div className="flex flex-col gap-1 border-t border-border bg-background p-2">
      {rows.map((row, index) => (
        <div key={index} className="flex flex-wrap justify-center gap-1">
          {row.map((key) => (
            <Button
              key={key.code}
              size="xs"
              variant={held.has(key.keysym) ? "primary" : "default"}
              className={cn("min-w-9", key === keys.space && "min-w-48")}
              onClick={() =>
                isModifier(key) ? onToggleModifier(key) : onPress(key)
              }
            >
              {key.label}
            </Button>
          ))}
        </div>
      ))}
    </div>
  );
}
