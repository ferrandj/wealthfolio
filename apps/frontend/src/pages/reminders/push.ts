import { getPushStatus, pushAvailable, subscribePush, unsubscribePush } from "@/adapters";
import type { PushStatus } from "@/lib/types/reminders";

/**
 * Whether this browser can be told about reminders when the app is closed.
 * - unavailable: not the web app, or a browser without notifications (on an iPhone, the app must first be
 *   added to the home screen), or a page not served over https
 * - denied: the person refused notifications for this site; only the browser's settings can change that
 * - off / on: can be turned on / is on for this device
 */
export type PushState = "unavailable" | "denied" | "off" | "on";

export function pushSupported(): boolean {
  return (
    pushAvailable &&
    typeof window !== "undefined" &&
    window.isSecureContext === true &&
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    "Notification" in window
  );
}

/** The key the server gives, as the bytes a browser subscribes with. */
export function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
  const base64 = base64url.replace(/-/g, "+").replace(/_/g, "/");
  const text = atob(base64 + "=".repeat((4 - (base64.length % 4)) % 4));
  const bytes = new Uint8Array(new ArrayBuffer(text.length));
  for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
  return bytes;
}

const base64url = (buffer: ArrayBuffer | null): string => {
  if (!buffer) return "";
  let text = "";
  for (const byte of new Uint8Array(buffer)) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

/** What the person will tell this device apart by: its browser and system. */
export function deviceLabel(userAgent: string = navigator.userAgent): string {
  const first = (names: [string, string][]) =>
    names.find(([mark]) => userAgent.includes(mark))?.[1] ?? "";
  const system = first([
    ["iPhone", "iOS"],
    ["iPad", "iOS"],
    ["Android", "Android"],
    ["Mac OS X", "macOS"],
    ["Windows", "Windows"],
    ["Linux", "Linux"],
  ]);
  // in this order: Edge and Chrome also say Safari, Edge also says Chrome
  const browser = first([
    ["Edg/", "Edge"],
    ["Firefox/", "Firefox"],
    ["Chrome/", "Chrome"],
    ["Safari/", "Safari"],
  ]);
  return [browser, system].filter(Boolean).join(" · ") || "Navigateur";
}

async function registration(): Promise<ServiceWorkerRegistration> {
  const existing = await navigator.serviceWorker.getRegistration("/");
  return existing ?? (await navigator.serviceWorker.register("/sw.js", { scope: "/" }));
}

/** Where this device stands, from the browser and from what the server keeps. */
export async function pushState(status?: PushStatus): Promise<PushState> {
  if (!pushSupported()) return "unavailable";
  if (Notification.permission === "denied") return "denied";
  const worker = await navigator.serviceWorker.getRegistration("/");
  const subscription = await worker?.pushManager.getSubscription();
  if (!subscription) return "off";
  const known = status ?? (await getPushStatus());
  return known.endpoints.includes(subscription.endpoint) ? "on" : "off";
}

/** Ask the person, subscribe this browser and give the subscription to the server. */
export async function enablePush(): Promise<PushState> {
  if (!pushSupported()) return "unavailable";
  const permission = await Notification.requestPermission();
  if (permission !== "granted") return permission === "denied" ? "denied" : "off";
  const worker = await registration();
  await navigator.serviceWorker.ready;
  const { publicKey } = await getPushStatus();
  // a subscription made for another server key cannot be used: start again
  const stale = await worker.pushManager.getSubscription();
  const staleKey = base64url(stale?.options.applicationServerKey ?? null);
  if (stale && staleKey !== publicKey) await stale.unsubscribe();
  const subscription =
    (stale && staleKey === publicKey ? stale : null) ??
    (await worker.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: keyBytes(publicKey),
    }));
  await subscribePush({
    endpoint: subscription.endpoint,
    p256dh: base64url(subscription.getKey("p256dh")),
    auth: base64url(subscription.getKey("auth")),
    label: deviceLabel(),
  });
  return "on";
}

/** Stop telling this device. */
export async function disablePush(): Promise<PushState> {
  if (!pushSupported()) return "unavailable";
  const worker = await navigator.serviceWorker.getRegistration("/");
  const subscription = await worker?.pushManager.getSubscription();
  if (subscription) {
    await unsubscribePush(subscription.endpoint);
    await subscription.unsubscribe();
  }
  return "off";
}
