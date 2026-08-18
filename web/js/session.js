// Being signed in, and stopping being signed in.
//
// The credential used to sit in localStorage and be pasted into an
// Authorization header on every call. It now lives in an HttpOnly
// cookie the server set, for two reasons.
//
// The first is that Safari deletes script-writable storage —
// localStorage and IndexedDB both — after seven days without a visit,
// and this platform asks people to turn up once a *month*. A member
// who did nothing wrong could open the site after a fortnight and find
// their membership gone. A cookie set over a first-party response is
// not swept that way.
//
// The second is that nothing in this file needs to read the credential
// at all. Cookies ride along on same-origin fetch automatically, so the
// page can be signed in without ever holding the thing that signs it
// in.
//
// What is left readable is the member number, in a second cookie. That
// is not a secret: it is the byline printed under everything they post.
// It is here so a page can tell whether somebody is a member, and who,
// without waiting on a request.
(() => {
  'use strict';

  const MEMBER = 'bf_member';

  function cookie(name) {
    return document.cookie
      .split(';')
      .map(p => p.trim())
      .filter(p => p.startsWith(name + '='))
      .map(p => p.slice(name.length + 1))
      .find(v => v.length) || null;
  }

  // The button appears on three pages with three stylesheets, and it is
  // the same button on all of them. Injected once, on first use, rather
  // than added to each — a control this small does not deserve three
  // copies that can drift apart.
  let styled = false;
  function style() {
    if (styled) return;
    styled = true;
    const s = document.createElement('style');
    s.textContent =
      '.bf-signout{font:inherit;font-size:inherit;color:inherit;background:none;' +
      'border:0;padding:0;cursor:pointer;text-decoration:underline;' +
      'text-underline-offset:3px;opacity:.75}' +
      '.bf-signout:hover{opacity:1}' +
      '.bf-signout[disabled]{cursor:default;opacity:.5}';
    document.head.appendChild(s);
  }

  const bf = {
    /// The member's number, or null when nobody is signed in here.
    memberNo() {
      const raw = cookie(MEMBER);
      const n = raw === null ? NaN : parseInt(raw, 10);
      return Number.isInteger(n) ? n : null;
    },

    signedIn() {
      return bf.memberNo() !== null;
    },

    /// Zero-padded, so a column of bylines lines up and an early number
    /// still reads as a number rather than a rank.
    label(n) {
      return 'no. ' + String(n).padStart(4, '0');
    },

    /// Hand the token from a /join link to the server and get cookies
    /// back. Resolves to the membership, or throws.
    async adopt(token) {
      const r = await fetch('/v1/members/session', {
        method: 'POST',
        headers: { 'Authorization': 'Bearer ' + token },
      });
      if (!r.ok) throw new Error('http ' + r.status);
      return r.json();
    },

    /// Give this browser up. Ends the session on the server too, so a
    /// token copied off the device before it was handed back is dead
    /// rather than merely forgotten.
    async signOut() {
      try {
        await fetch('/v1/members/session', { method: 'DELETE' });
      } catch { /* offline: clearing the page state is still right */ }
      // The chat keys are non-extractable and specific to this browser.
      // Leaving them behind would mean the next person to sign in here
      // holds a key they can never use, so they go with the session.
      try {
        indexedDB.deleteDatabase('box-fraise-chat');
      } catch { /* nothing to delete */ }
    },

    /// A sign-out control, appended to whatever element is passed.
    ///
    /// Warns first. Signing out on a phone is not recoverable from the
    /// phone: the credential was shown once, as a QR, and only its hash
    /// was kept. Getting back in means going to a run — which is the
    /// design, but it should not be a surprise.
    mountSignOut(el) {
      if (!el) return;
      style();
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'bf-signout';
      b.textContent = 'sign out';
      b.addEventListener('click', async () => {
        const ok = confirm(
          'Sign out of this browser?\n\n' +
          'There is no password to sign back in with. Your number, your posts and ' +
          'your attendance all stay — but to use them again you have to come to a ' +
          'run and have somebody hand you a new code.\n\n' +
          'Your conversations stay on this device and cannot be moved, so they ' +
          'will not come back.');
        if (!ok) return;
        b.disabled = true;
        b.textContent = 'signing out…';
        await bf.signOut();
        location.href = '/';
      });
      el.appendChild(b);
    },
  };

  window.bfSession = bf;
})();
