// Service worker. It exists for two reasons: Chrome will not offer to
// install a site without one, and push messages are delivered to it.
// Caching is the secondary job and is deliberately timid.
//
// NETWORK FIRST, everywhere. This site has no build step and no hashed
// filenames, and it is deployed several times a day — a cache-first
// worker would hand somebody a page from last week and there would be
// no way to tell them to clear it. The cache is an offline fallback,
// not a speed-up.
//
// THREE PATHS ARE NEVER CACHED, and the reasons are not performance:
//
//   /v1/*   Member data behind row-level security. A cached response
//           outlives the session that was allowed to read it, so a
//           sign-out or a credential re-issue would leave the old
//           member's feed sitting on the device.
//   /join   The URL itself carries the credential. It is noindex for
//           the same reason; a copy in the cache is a copy of the key.
//   /admin  Someone else's power over other people's accounts.
//
// If you add another authenticated path, add it to BYPASS. The default
// is to cache, so forgetting is the dangerous direction.
const CACHE = 'bf-v1';
const BYPASS = ['/v1/', '/join', '/admin'];

self.addEventListener('install', (event) => {
  // Take over immediately rather than waiting for every tab to close.
  // Without a versioned build there is no other way to guarantee a
  // deploy reaches somebody who keeps the app open.
  event.waitUntil(self.skipWaiting());
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;

  const url = new URL(req.url);
  // Cross-origin (the fonts) is left entirely alone. Opaque responses
  // cannot be inspected, so caching them is caching something unread.
  if (url.origin !== self.location.origin) return;
  if (BYPASS.some((p) => url.pathname.startsWith(p))) return;

  event.respondWith(
    fetch(req)
      .then((res) => {
        // Only store a real answer. A 404 or a 500 kept in the cache
        // would be served back after the site recovered.
        if (res && res.ok && res.type === 'basic') {
          const copy = res.clone();
          caches.open(CACHE).then((c) => c.put(req, copy));
        }
        return res;
      })
      .catch(() =>
        caches.match(req).then((hit) => {
          if (hit) return hit;
          // An installed app opened cold on a dead train still gets
          // the homepage rather than a browser error page.
          if (req.mode === 'navigate') return caches.match('/');
          return Response.error();
        })
      )
  );
});
