// Registers the service worker. Loaded by every page that carries the
// manifest link, so an install can be started from wherever somebody
// happens to have landed rather than only from the homepage.
//
// Deliberately silent: iOS never offers to install a web app, so there
// is nothing to prompt with and no banner worth writing. Adding the
// icon is something an admin shows a member how to do at a run, which
// is how every other part of this platform works.
(() => {
  'use strict';
  if (!('serviceWorker' in navigator)) return;
  // Registration is not urgent and competes with the page for
  // bandwidth on a phone in a park.
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {
      // Private browsing and some enterprise profiles refuse. The site
      // works without it; only install and push are lost.
    });
  });
})();
