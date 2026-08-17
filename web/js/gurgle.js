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

  window.wireGurgle = function wireGurgle() {
    const form = document.getElementById('gg-form');
    if (!form) return;

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
        if (title) fd.append('title', title);
        if (body)  fd.append('body', body);
        const name    = document.getElementById('gg-name').value.trim();
        const contact = document.getElementById('gg-contact').value.trim();
        if (name)    fd.append('submitter_name', name);
        if (contact) fd.append('submitter_contact', contact);
        if (file) {
          const blob = await prepareImage(file);
          fd.append('image', blob, 'photograph.jpg');
        }

        msgEl.textContent = 'Sending…';
        const r = await fetch('/v1/submissions', { method: 'POST', body: fd });
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
