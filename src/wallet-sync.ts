import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { WalletSnapshot } from "./runtime";

// Subscribe before reading the cache; a late initial read must not overwrite a
// newer pushed snapshot. All account views consume the same native refresh.
export async function watchWallet(update: (snapshot: WalletSnapshot) => void): Promise<() => void> {
  return watchSnapshot("wallet-updated", "get_wallet_status", update);
}
async function watchSnapshot<T>(eventName: string, command: string, update: (snapshot: T) => void): Promise<() => void> {
  let receivedEvent = false;
  const stop = await listen<T>(eventName, event => {
    receivedEvent = true;
    update(event.payload);
  });
  try {
    const initial = await invoke<T>(command);
    if (!receivedEvent) update(initial);
  } catch {
    // Keep listening: the next background update recovers without reopening.
  }
  return stop;
}
