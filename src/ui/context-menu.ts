import { Menu } from "@tauri-apps/api/menu";
import type { AppSettings } from "../domain/types";

export interface WidgetMenuActions {
  update: (patch: Partial<AppSettings>) => Promise<void>;
  setMousePassthrough: (enabled: boolean) => Promise<void>;
  openSettings: () => Promise<void>;
  hide: () => Promise<void>;
  exit: () => Promise<void>;
}

export async function showWidgetContextMenu(
  settings: AppSettings,
  mousePassthrough: boolean,
  actions: WidgetMenuActions,
): Promise<void> {
  const menu = await Menu.new({
    items: [
      {
        id: "always-on-top",
        text: "始终置顶",
        checked: settings.alwaysOnTop,
        action: () => void actions.update({ alwaysOnTop: !settings.alwaysOnTop }),
      },
      {
        id: "lock-size",
        text: "锁定大小",
        checked: settings.lockSize || settings.lockWindow,
        enabled: !settings.lockWindow,
        action: () => void actions.update({ lockSize: !settings.lockSize }),
      },
      {
        id: "lock-window",
        text: "锁定窗口",
        checked: settings.lockWindow,
        action: () => void actions.update({ lockWindow: !settings.lockWindow }),
      },
      {
        id: "mouse-passthrough",
        text: "鼠标穿透",
        checked: mousePassthrough,
        action: () => void actions.setMousePassthrough(!mousePassthrough),
      },
      { item: "Separator" },
      { id: "settings", text: "设置…", action: () => void actions.openSettings() },
      { id: "hide", text: "隐藏到托盘", action: () => void actions.hide() },
      { item: "Separator" },
      { id: "exit", text: "退出", action: () => void actions.exit() },
    ],
  });
  await menu.popup();
}
