// Shows the notifications the server sends (a document to bring back, rates to confirm), the app open or not,
// and opens the app on the page a notification is about. It keeps nothing and caches nothing.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("push", (event) => {
  let told = {};
  try {
    told = event.data ? event.data.json() : {};
  } catch {
    told = { body: event.data ? event.data.text() : "" };
  }
  event.waitUntil(
    self.registration.showNotification(told.title || "Wealthfolio", {
      body: told.body || "",
      tag: told.tag || "wealthfolio",
      icon: "/app-icon-192.png",
      badge: "/app-icon-192.png",
      data: { url: told.url || "/reminders" },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = new URL(event.notification.data?.url || "/reminders", self.location.origin).href;
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((windows) => {
      const open = windows.find((w) => new URL(w.url).origin === self.location.origin);
      if (open) {
        return open.focus().then((w) => ("navigate" in w ? w.navigate(url) : undefined));
      }
      return self.clients.openWindow(url);
    }),
  );
});
