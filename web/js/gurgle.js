// gurgle — sending a column or a photograph in to the magazine.
//
// Shared by /gurgle (the page anyone can reach, including on a phone)
// and by the gurgle.app window on /os. One implementation, two hosts:
// the validation, the image handling and the wire format have to agree
// with the server, and two copies of that would drift.
//
// Both hosts use the same element ids. Call wireGurgle() once the DOM
// is ready; it does nothing if the form is not on the page.
(() => {
  'use strict';

  // Whether this browser holds a membership, put here by /join when a
  // code was scanned off an admin's screen at the run club. Its absence
  // is simply not being a member.
  //
  // No token is read anywhere below: the credential is an HttpOnly
  // cookie the server set, and same-origin fetch sends it without being
  // asked. See web/js/session.js, which must be loaded first.
  const member = () => window.bfSession && window.bfSession.signedIn();

  // The three live in web/js/prompts.js, which must load first.


  // Mirrors MIN_BODY_CHARS in src/domain/submissions/service.rs. A
  // column of two words is a mistake or a probe, not a column.
  const MIN_BODY = 40;
  // Photographs go through a canvas before they are sent: it caps the
  // long edge, drops the file well under the server's 8 MB ceiling,
  // and turns anything the browser can decode — HEIC on Safari
  // included — into the JPEG the server will accept.
  const MAX_EDGE = 2000;

  function prepareImage(file) {
    return new Promise((resolve, reject) => {
      const url = URL.createObjectURL(file);
      const img = new Image();
      img.onload = () => {
        URL.revokeObjectURL(url);
        const scale = Math.min(1, MAX_EDGE / Math.max(img.width, img.height));
        const c = document.createElement('canvas');
        c.width  = Math.round(img.width  * scale);
        c.height = Math.round(img.height * scale);
        c.getContext('2d').drawImage(img, 0, 0, c.width, c.height);
        c.toBlob(b => b ? resolve(b) : reject(new Error('could not read that image')),
                 'image/jpeg', 0.85);
      };
      img.onerror = () => {
        URL.revokeObjectURL(url);
        reject(new Error('that image could not be read — try a JPEG or PNG'));
      };
      img.src = url;
    });
  }

  // The post button. There is one way to become a member of this
  // platform and it is turning up to the run club, so until somebody
  // has, pressing this explains where memberships come from rather
  // than opening a form nobody is allowed to submit.
  // A member's standing, straight from the same functions the INSERT
  // policy uses — so what somebody is told and what the database will
  // allow cannot disagree.
  window.renderMembership = async function renderMembership(elId) {
    const el = document.getElementById(elId);
    if (!el || !member()) return;
    try {
      const r = await fetch('/v1/members/me');
      if (!r.ok) return;
      const m = await r.json();
      const no = window.bfSession.label(m.member_no);
      const until = m.current_until && new Date(m.current_until).toLocaleDateString(
        undefined, { month: 'short', day: 'numeric' });

      el.textContent = '';
      const line = document.createElement('span');
      line.textContent = m.current
        ? no + ' · posting until ' + until + ', then come for a run'
        : no + ' · lapsed. Come for a run and it comes back.';
      el.appendChild(line);
      el.appendChild(document.createTextNode(' · '));
      // The way off this device. It sits next to the number rather than
      // in a menu, because the case it exists for is handing a phone to
      // somebody, and that happens in a hurry.
      window.bfSession.mountSignOut(el);
      el.hidden = false;
    } catch { /* silent: it is a status line, not the page */ }
  };

  window.wireGurgle = function wireGurgle() {
    const open   = document.getElementById('gg-open');
    const panel  = document.getElementById('gg-panel');
    const form   = document.getElementById('gg-form');

    // A member gets the form; everybody else gets told where
    // memberships come from.
    const isMember = member();
    const target = isMember ? form : panel;
    if (open && target) {
      open.addEventListener('click', () => {
        const showing = !target.hidden;
        target.hidden = showing;
        open.textContent = showing ? 'post, or whatever' : 'never mind';
        open.setAttribute('aria-expanded', String(!showing));
        if (!showing && isMember) target.querySelector('textarea').focus();
      });
    }

    if (!form || !isMember) return;

    // Which of the three is being answered. Chosen before writing,
    // because what you are answering changes what you write.
    const holder = document.createElement('div');
    form.insertBefore(holder, form.firstChild);
    const chosenPrompt = window.bfPromptPicker(holder);

    const bodyEl  = document.getElementById('gg-body');
    const msgEl   = document.getElementById('gg-msg');
    const sendEl  = document.getElementById('gg-send');
    const countEl = document.getElementById('gg-count');

    bodyEl.addEventListener('input', () => {
      const n = bodyEl.value.trim().length;
      countEl.textContent = n === 0 ? '0'
        : n < MIN_BODY ? n + ' — a column needs ' + MIN_BODY
        : String(n);
    });

    form.addEventListener('submit', async e => {
      e.preventDefault();
      const body  = bodyEl.value.trim();
      const file  = document.getElementById('gg-image').files[0];
      const title = document.getElementById('gg-title').value.trim();

      // Say no here rather than making the server say it.
      if (!body && !file) {
        msgEl.className = 'msg err';
        msgEl.textContent = 'Send a column, a photograph, or both.';
        return;
      }
      if (body && body.length < MIN_BODY) {
        msgEl.className = 'msg err';
        msgEl.textContent = 'A column needs at least ' + MIN_BODY + ' characters.';
        return;
      }
      if (title && !body) {
        msgEl.className = 'msg err';
        msgEl.textContent = 'A title needs a column under it.';
        return;
      }
      sendEl.disabled = true;
      msgEl.className = 'msg err';
      msgEl.textContent = file ? 'Preparing the photograph…' : 'Sending…';

      try {
        const fd = new FormData();
        fd.append('prompt', chosenPrompt());
        if (title) fd.append('title', title);
        if (body)  fd.append('body', body);
        if (file) {
          const blob = await prepareImage(file);
          fd.append('image', blob, 'photograph.jpg');
        }

        msgEl.textContent = 'Sending…';
        const r = await fetch('/v1/submissions', {
          method: 'POST',
          body: fd,
        });
        const text = await r.text();
        let parsed = null;
        if (text) { try { parsed = JSON.parse(text); } catch { /* leave null */ } }
        if (!r.ok) {
          throw new Error((parsed && (parsed.message || parsed.error)) || text || ('http ' + r.status));
        }

        form.reset();
        countEl.textContent = '0';
        msgEl.className = 'msg ok';
        msgEl.textContent = 'Sent. It is with the editor now.';
      } catch (err) {
        msgEl.className = 'msg err';
        msgEl.textContent = err.message || 'It did not send. Try again in a moment.';
      } finally {
        sendEl.disabled = false;
      }
    });
  };
})();

// ── The feed ────────────────────────────────────────────────────────
// What the editor has put up, newest first. Shared by /gurgle and by
// the gurgle.app window, which render it at different sizes but from
// the same data.
//
// Everything a stranger typed goes in through textContent. A post is
// arbitrary text from an open write path, and the one place it is
// displayed is the one place that matters.
(() => {
  'use strict';

  // Zero-padded so the column of bylines lines up and an early
  // number still reads as a number rather than a rank.
  const memberNo = n => 'no. ' + String(n).padStart(4, '0');

  window.renderGurgleFeed = async function renderGurgleFeed(rootId) {
    const root = document.getElementById(rootId);
    if (!root) return;

    let posts;
    try {
      const r = await fetch('/v1/submissions/published');
      if (!r.ok) throw new Error('http ' + r.status);
      posts = await r.json();
    } catch (e) {
      console.error(e);
      root.textContent = 'The feed could not be reached.';
      return;
    }

    root.textContent = '';
    if (!posts.length) {
      const p = document.createElement('p');
      p.className = 'gf-empty';
      p.textContent = 'Nothing has gone up yet.';
      root.appendChild(p);
      return;
    }

    posts.forEach(post => {
      const el = document.createElement('article');
      el.className = 'gf-post';

      // What it is an answer to. Without this the feed is three
      // conversations printed on top of each other.
      const q = document.createElement('p');
      q.className = 'gf-prompt';
      q.textContent = (window.bfPromptLabel && window.bfPromptLabel(post.prompt)) || '';
      if (q.textContent) el.appendChild(q);

      if (post.title) {
        const h = document.createElement('h3');
        h.className = 'gf-title';
        h.textContent = post.title;
        el.appendChild(h);
      }

      if (post.has_image) {
        const img = document.createElement('img');
        img.className = 'gf-img';
        img.loading = 'lazy';
        img.alt = post.title || 'A photograph';
        img.src = '/v1/submissions/published/' + post.id + '/image';
        el.appendChild(img);
      }

      if (post.body) {
        const b = document.createElement('p');
        b.className = 'gf-body';
        b.textContent = post.body;
        el.appendChild(b);
      }

      // A member is a number, not a name. Nothing public about
      // somebody here was chosen by them.
      const by = document.createElement('p');
      by.className = 'gf-by';
      by.textContent = memberNo(post.member_no) + ' · ' +
        new Date(post.published_at).toLocaleDateString(undefined,
          { year: 'numeric', month: 'short', day: 'numeric' });
      el.appendChild(by);

      root.appendChild(el);
    });
  };
})();
