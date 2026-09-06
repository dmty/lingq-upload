import { goto } from "$app/navigation";

export const SETTINGS_WINDOW_LABEL = "settings";

/**
 * Settings are not content, so they open in their own window from ⌘, rather
 * than as a destination in the sidebar. Outside Tauri (browser, e2e) there are
 * no windows to open and the `/settings` route stands in for one.
 */
export async function openSettings(): Promise<void> {
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(SETTINGS_WINDOW_LABEL);
    if (existing) {
      await existing.show();
      await existing.setFocus();
      return;
    }
    // Plain decorations, unlike the main window: a Mac settings window keeps
    // its title bar, and there is no sidebar here to run the traffic lights
    // into.
    new WebviewWindow(SETTINGS_WINDOW_LABEL, {
      url: "/settings",
      title: "Settings",
      width: 560,
      height: 620,
      minWidth: 480,
      minHeight: 400,
      resizable: true,
    });
  } catch {
    await goto("/settings");
  }
}
