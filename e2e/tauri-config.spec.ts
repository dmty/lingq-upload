import { expect, test } from "@playwright/test";
import capabilities from "../src-tauri/capabilities/default.json" with { type: "json" };
import config from "../src-tauri/tauri.conf.json" with { type: "json" };

test.describe("macOS window configuration", () => {
  test("transparency is enabled the way macOS requires", () => {
    // windowEffects installs an NSVisualEffectView below the webview; without
    // this flag the webview stays opaque and hides it.
    expect(config.app.macOSPrivateApi).toBe(true);
    expect(config.app.windows[0].transparent).toBe(true);
  });

  test("the titlebar overlays the content", () => {
    const w = config.app.windows[0];
    expect(w.titleBarStyle).toBe("Overlay");
    expect(w.hiddenTitle).toBe(true);
    expect(w.trafficLightPosition).toEqual({ x: 14, y: 19 });
  });

  test("the sidebar requests vibrancy", () => {
    expect(config.app.windows[0].windowEffects.effects).toContain("sidebar");
  });

  test("the drag region is permitted to drag", () => {
    // core:default does NOT include this; without it the strip is inert
    // while double-click-to-zoom still works, which masks the failure.
    expect(capabilities.permissions).toContain(
      "core:window:allow-start-dragging",
    );
  });
});
