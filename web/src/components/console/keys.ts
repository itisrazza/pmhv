/** A key as both VNC (X11 keysym + DOM code) and serial (bytes, where it has any) understand it. */
export interface Key {
  label: string;
  keysym: number;
  code: string;
}

export const keys = {
  ctrl: { label: "Ctrl", keysym: 0xffe3, code: "ControlLeft" },
  alt: { label: "Alt", keysym: 0xffe9, code: "AltLeft" },
  shift: { label: "Shift", keysym: 0xffe1, code: "ShiftLeft" },
  meta: { label: "Cmd / Win", keysym: 0xffeb, code: "MetaLeft" },
  tab: { label: "Tab", keysym: 0xff09, code: "Tab" },
  escape: { label: "Esc", keysym: 0xff1b, code: "Escape" },
  delete: { label: "Del", keysym: 0xffff, code: "Delete" },
  backspace: { label: "Backspace", keysym: 0xff08, code: "Backspace" },
  enter: { label: "Enter", keysym: 0xff0d, code: "Enter" },
  space: { label: "Space", keysym: 0x0020, code: "Space" },
  printScreen: { label: "PrtSc / SysRq", keysym: 0xff61, code: "PrintScreen" },
  up: { label: "↑", keysym: 0xff52, code: "ArrowUp" },
  down: { label: "↓", keysym: 0xff54, code: "ArrowDown" },
  left: { label: "←", keysym: 0xff51, code: "ArrowLeft" },
  right: { label: "→", keysym: 0xff53, code: "ArrowRight" },
} satisfies Record<string, Key>;

export function functionKey(number: number): Key {
  return {
    label: `F${number}`,
    keysym: 0xffbe + number - 1,
    code: `F${number}`,
  };
}

export function letterKey(letter: string): Key {
  const lower = letter.toLowerCase();
  return {
    label: letter.toUpperCase(),
    keysym: lower.charCodeAt(0),
    code: `Key${letter.toUpperCase()}`,
  };
}

export function isModifier(key: Key): boolean {
  return [keys.ctrl, keys.alt, keys.shift, keys.meta].some(
    (modifier) => modifier.keysym === key.keysym,
  );
}

export interface Combo {
  label: string;
  keys: Key[];
}

/** Combos the browser or host OS would otherwise swallow. */
export const graphicalCombos: Combo[] = [
  { label: "Ctrl+Alt+Del", keys: [keys.ctrl, keys.alt, keys.delete] },
  { label: "Cmd / Win", keys: [keys.meta] },
  { label: "Alt+Tab", keys: [keys.alt, keys.tab] },
  { label: "Cmd / Win+Tab", keys: [keys.meta, keys.tab] },
  { label: "Alt+F4", keys: [keys.alt, functionKey(4)] },
  { label: "PrtSc", keys: [keys.printScreen] },
  ...Array.from({ length: 6 }, (_, index) => ({
    label: `Ctrl+Alt+F${index + 1}`,
    keys: [keys.ctrl, keys.alt, functionKey(index + 1)],
  })),
];

/** Control characters for a serial console, where only bytes exist. */
export const textCombos: { label: string; bytes: string }[] = [
  { label: "Ctrl+C", bytes: "\x03" },
  { label: "Ctrl+D", bytes: "\x04" },
  { label: "Ctrl+Z", bytes: "\x1a" },
  { label: "Ctrl+\\", bytes: "\x1c" },
  { label: "Ctrl+A", bytes: "\x01" },
  { label: "Ctrl+]", bytes: "\x1d" },
  { label: "Esc", bytes: "\x1b" },
  { label: "Tab", bytes: "\t" },
];
