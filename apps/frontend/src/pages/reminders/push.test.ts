import { describe, expect, it, vi } from "vitest";

vi.mock("@/adapters", () => ({
  pushAvailable: true,
  getPushStatus: vi.fn(),
  subscribePush: vi.fn(),
  unsubscribePush: vi.fn(),
}));

import { deviceLabel, keyBytes, pushState, pushSupported } from "./push";

describe("push on this browser", () => {
  it("reads the server's key as the bytes a browser subscribes with", () => {
    // base64url, unpadded: "-_8" is the three bytes fb ff
    expect([...keyBytes("-_8")]).toEqual([0xfb, 0xff]);
    expect(
      keyBytes(
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
      ),
    ).toHaveLength(65);
  });

  it("names a device by its browser and system", () => {
    expect(
      deviceLabel(
        "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Mobile Safari/537.36",
      ),
    ).toBe("Chrome · Android");
    expect(
      deviceLabel(
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
      ),
    ).toBe("Safari · iOS");
    expect(
      deviceLabel(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0",
      ),
    ).toBe("Firefox · Windows");
  });

  it("is unavailable where the browser has no notifications", async () => {
    // jsdom has neither a service worker nor a push manager
    expect(pushSupported()).toBe(false);
    expect(await pushState()).toBe("unavailable");
  });
});
