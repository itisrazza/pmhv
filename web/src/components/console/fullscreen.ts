/** The Keyboard Lock API, Chromium-only, so not in TypeScript's DOM types. */
interface KeyboardLock {
  lock: (codes?: string[]) => Promise<void>;
  unlock: () => void;
}

function keyboardLock(): KeyboardLock | undefined {
  return (navigator as Navigator & { keyboard?: KeyboardLock }).keyboard;
}

export function keyboardLockSupported(): boolean {
  return keyboardLock() !== undefined;
}

/**
 * Goes fullscreen and, where the browser allows it, captures system shortcuts (Cmd+Tab,
 * Alt+Tab, the Windows key…) so they reach the guest. Holding Esc leaves fullscreen.
 */
export async function enterFullscreen(element: HTMLElement): Promise<void> {
  await element.requestFullscreen();
  await keyboardLock()?.lock();
}

export async function exitFullscreen(): Promise<void> {
  keyboardLock()?.unlock();
  if (document.fullscreenElement) {
    await document.exitFullscreen();
  }
}
