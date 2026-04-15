// Minimal service worker.
//
// Exists solely to satisfy Chrome's PWA installability criteria, which are
// required for `navigator.storage.persist()` to auto-grant. Without a
// registered service worker that has a `fetch` handler, Chrome refuses to
// upgrade our IndexedDB to persistent storage — leaving user identity keys
// vulnerable to eviction under disk pressure.
//
// We do NOT implement offline caching, precaching, or any request
// interception beyond pass-through. If full PWA offline support is wanted
// later, reach for `vite-plugin-pwa` + Workbox rather than extending this.

self.addEventListener("install", () => {
  // Activate on next navigation, skip the waiting step.
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  // Take control of existing clients immediately so the first install is
  // also the first active registration.
  event.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", () => {
  // Required by Chrome's installability check — we don't actually
  // intercept anything, so just let the network handle every request.
});
