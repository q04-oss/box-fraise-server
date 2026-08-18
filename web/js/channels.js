// Channels — meeting somebody, and then talking to them.
//
// This was the whole of /chat, a page of its own reached from the
// homepage nav. That was wrong twice over: it offered strangers a
// top-level link to something only members can open, and it put a
// member's conversations somewhere other than where their number, their
// feed and their post button already are. Channels belong inside
// gurgle.
//
// Extracted here rather than copied into two pages, for the same reason
// gurgle.js exists: /gurgle and the gurgle.app window on /os render it
// at different sizes from the same code, and two copies of a pairing
// flow would drift apart.
//
// Call renderChannels(elementId) once. It builds its own markup, so a
// host only has to provide an empty container and have loaded
// session.js, chat.js and qrcode.min.js first.
//
// Nothing here holds a credential and nothing here can read a message.
// The words are decrypted in bfChat, from a key derived in this browser
// — see js/chat.js.
(() => {
  'use strict';

  // Scoped under .bf-ch so a host's own rules cannot reach in and so
  // this cannot reach out. The custom properties fall back to the same
  // values both hosts already define, so a third host would still look
  // right without being told to.
  const CSS = `
.bf-ch { --ch-ink: var(--ink, #1a1a1a); --ch-rule: var(--rule, #e6e6e6);
         --ch-muted: var(--muted, #666); --ch-accent: var(--accent, #b21b1b);
         --ch-mono: 'IBM Plex Mono', ui-monospace, Menlo, monospace; }
.bf-ch .ch-kv { font-family: var(--ch-mono); font-size: 10px; text-transform: uppercase;
                letter-spacing: 0.14em; color: var(--ch-muted); margin: 0 0 12px; }

/* Meeting somebody. Two buttons, because there are two sides of the
   same act and only one of you needs to hold the phone. */
.bf-ch .ch-meet { display: flex; gap: 10px; flex-wrap: wrap; margin: 0 0 8px; }
.bf-ch .ch-meet button {
  font-family: var(--ch-mono); font-size: 11px; text-transform: lowercase;
  letter-spacing: 0.1em; padding: 14px 22px; border: 1px solid var(--ch-ink);
  border-radius: 999px; background: none; color: var(--ch-ink); cursor: pointer;
}
.bf-ch .ch-meet button:hover { border-color: var(--ch-accent); color: var(--ch-accent); }
.bf-ch .ch-code, .bf-ch .ch-scan { display: none; margin: 16px 0 26px; }
.bf-ch .ch-code.on, .bf-ch .ch-scan.on { display: block; }
.bf-ch .ch-qr { background: #fff; display: inline-block; }
.bf-ch .ch-qr img { display: block; }
.bf-ch .ch-cam { width: 100%; max-width: 320px; border-radius: 8px; background: #000; display: block; }
.bf-ch .ch-nonce { font-family: var(--ch-mono); font-size: 12px; letter-spacing: 0.08em;
                   word-break: break-all; color: var(--ch-muted); margin-top: 10px; }
.bf-ch .ch-paste { width: 100%; font: inherit; font-size: 15px; margin-top: 10px;
                   border: 1px solid var(--ch-rule); border-radius: 8px; padding: 10px; }

/* One row per pairing. Only the open ones can be entered; the rest are
   shown because knowing a channel is coming is part of it. */
.bf-ch .ch-row {
  display: flex; align-items: baseline; gap: 12px; width: 100%;
  border: 0; border-bottom: 1px solid var(--ch-rule); padding: 15px 0;
  text-align: left; font: inherit; color: var(--ch-ink); background: none;
}
.bf-ch .ch-row[data-open="1"] { cursor: pointer; }
.bf-ch .ch-row[data-open="1"]:hover { border-bottom-color: var(--ch-ink); }
.bf-ch .ch-row .who { flex: 1; font-size: 17px; }
.bf-ch .ch-row .st { font-family: var(--ch-mono); font-size: 9px; text-transform: uppercase;
                     letter-spacing: 0.13em; color: var(--ch-muted); flex: 0 0 auto; }
.bf-ch .ch-row[data-open="1"] .st { color: var(--ch-accent); }
.bf-ch .ch-decide { display: flex; gap: 8px; padding: 0 0 15px; }
.bf-ch .ch-decide button {
  font-family: var(--ch-mono); font-size: 10px; text-transform: uppercase;
  letter-spacing: 0.1em; padding: 7px 12px; border-radius: 999px; cursor: pointer;
  border: 1px solid var(--ch-rule); background: none; color: var(--ch-ink);
}
.bf-ch .ch-decide button.yes { border-color: var(--ch-accent); color: var(--ch-accent); }

/* The conversation. */
.bf-ch .ch-room { display: none; }
.bf-ch .ch-room.on { display: block; }
.bf-ch .ch-list { border-top: 2px solid var(--ch-ink); margin: 18px 0 0; }
.bf-ch .ch-msg { padding: 12px 0; border-bottom: 1px solid var(--ch-rule); }
.bf-ch .ch-msg .from { font-family: var(--ch-mono); font-size: 9px; text-transform: uppercase;
                       letter-spacing: 0.13em; color: var(--ch-muted); margin-bottom: 5px; }
.bf-ch .ch-msg.mine .from { color: var(--ch-accent); }
.bf-ch .ch-msg .txt { font-size: 16px; line-height: 1.55; white-space: pre-wrap; }
.bf-ch .ch-msg .txt.lost { color: var(--ch-muted); font-style: italic; }
.bf-ch .ch-draft {
  width: 100%; font: inherit; font-size: 16px; margin-top: 18px;
  border: 1px solid var(--ch-rule); border-radius: 8px; padding: 12px;
  min-height: 88px; resize: vertical;
}
.bf-ch .ch-draft:focus { outline: none; border-color: var(--ch-accent); }
.bf-ch .ch-send {
  font-family: var(--ch-mono); font-size: 11px; text-transform: uppercase;
  letter-spacing: 0.14em; padding: 14px 24px; border: 0; border-radius: 999px;
  background: var(--ch-accent); color: #fff; cursor: pointer; margin-top: 12px;
}
.bf-ch .ch-send[disabled] { background: #ccc; cursor: default; }
.bf-ch .ch-line { font-size: 14px; color: var(--ch-muted); margin-top: 10px; }
.bf-ch .ch-empty { font-size: 17px; line-height: 1.6; color: var(--ch-muted); margin: 0; }
.bf-ch .ch-fine { font-size: 13px; line-height: 1.6; color: var(--ch-muted); margin-top: 28px; }
`;

  let styled = false;
  function injectStyles() {
    if (styled) return;
    styled = true;
    const s = document.createElement('style');
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  /// Small DOM helper. Every string that reaches a user goes in as
  /// text, never as markup — a channel carries what somebody typed.
  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  }

  window.renderChannels = function renderChannels(rootId) {
    const root = document.getElementById(rootId);
    if (!root || !window.bfSession || !bfSession.signedIn()) return;
    injectStyles();

    // ── Markup ───────────────────────────────────────────────────
    root.classList.add('bf-ch');
    root.textContent = '';

    root.appendChild(el('p', 'ch-kv', 'Meet somebody'));
    const meet = el('div', 'ch-meet');
    const showBtn = el('button', null, 'show my code');
    const scanBtn = el('button', null, 'scan a code');
    showBtn.type = scanBtn.type = 'button';
    meet.appendChild(showBtn);
    meet.appendChild(scanBtn);
    root.appendChild(meet);

    const codeBox = el('div', 'ch-code');
    const qrHolder = el('div', 'ch-qr');
    const nonceText = el('p', 'ch-nonce');
    codeBox.appendChild(qrHolder);
    codeBox.appendChild(el('p', 'ch-line', 'Have them scan this. It lasts two minutes.'));
    codeBox.appendChild(nonceText);
    root.appendChild(codeBox);

    const scanBox = el('div', 'ch-scan');
    const cam = el('video', 'ch-cam');
    cam.autoplay = cam.playsInline = cam.muted = true;
    cam.setAttribute('playsinline', '');
    const scanMsg = el('p', 'ch-line', 'Point it at their code.');
    const paste = el('input', 'ch-paste');
    paste.placeholder = 'or type the code out';
    scanBox.appendChild(cam);
    scanBox.appendChild(scanMsg);
    scanBox.appendChild(paste);
    root.appendChild(scanBox);

    root.appendChild(el('p', 'ch-kv', 'Yours'));
    const chans = el('div', 'ch-chans');
    root.appendChild(chans);

    const room = el('div', 'ch-room');
    const roomWho = el('p', 'ch-kv');
    const list = el('div', 'ch-list');
    const draft = el('textarea', 'ch-draft');
    draft.maxLength = 4000;
    draft.placeholder = 'Say something.';
    const sendBtn = el('button', 'ch-send', 'Send');
    sendBtn.type = 'button';
    const sendMsg = el('div', 'ch-line');
    room.appendChild(roomWho);
    room.appendChild(list);
    room.appendChild(draft);
    room.appendChild(el('div')).appendChild(sendBtn);
    room.appendChild(sendMsg);
    root.appendChild(room);

    root.appendChild(el('p', 'ch-fine',
      'Encrypted between the two phones. The server keeps ciphertext and has no key, so it ' +
      'can see that two numbers exchanged something and when — never what. Losing the phone ' +
      'loses the conversation: there is no copy anywhere to restore from.'));

    // ── Meeting somebody ─────────────────────────────────────────
    // One of you shows a code and the other reads it. Which way round
    // does not matter: the pairing is the same either way, and both
    // people had to be there for it to happen at all.
    let scanning = null;
    let current = null;

    showBtn.addEventListener('click', async () => {
      stopScanning();
      scanBox.classList.remove('on');
      codeBox.classList.add('on');
      nonceText.textContent = 'asking…';
      try {
        const r = await fetch('/v1/pairings/nonce', { method: 'POST' });
        if (!r.ok) throw new Error('http ' + r.status);
        const { nonce } = await r.json();
        qrHolder.textContent = '';
        const qr = qrcode(0, 'M');
        qr.addData(nonce);
        qr.make();
        qrHolder.innerHTML = qr.createImgTag(6, 8);
        // Shown as text too: a cracked screen or a dim camera should
        // not be the reason two people cannot connect.
        nonceText.textContent = nonce;
      } catch {
        nonceText.textContent = 'Could not get a code.';
      }
    });

    scanBtn.addEventListener('click', async () => {
      codeBox.classList.remove('on');
      scanBox.classList.add('on');

      if (!('BarcodeDetector' in window) || !navigator.mediaDevices) {
        scanMsg.textContent = 'This browser cannot use the camera — type the code instead.';
        return;
      }
      try {
        const stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: 'environment' },
        });
        cam.srcObject = stream;
        await cam.play().catch(() => {});
        const detector = new BarcodeDetector({ formats: ['qr_code'] });
        scanning = { stream, timer: setInterval(async () => {
          try {
            const found = await detector.detect(cam);
            if (found.length) await pairWith(found[0].rawValue);
          } catch { /* a frame that did not decode */ }
        }, 400) };
      } catch {
        scanMsg.textContent = 'No camera — type the code instead.';
      }
    });

    paste.addEventListener('keydown', e => {
      if (e.key === 'Enter' && e.target.value.trim()) pairWith(e.target.value.trim());
    });

    function stopScanning() {
      if (!scanning) return;
      clearInterval(scanning.timer);
      scanning.stream.getTracks().forEach(t => t.stop());
      scanning = null;
    }

    let pairing = false;
    async function pairWith(nonce) {
      if (pairing) return;
      pairing = true;
      scanMsg.textContent = 'Pairing…';
      try {
        const r = await fetch('/v1/pairings/claim-in-person', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ nonce }),
        });
        const text = await r.text();
        let body = null;
        if (text) { try { body = JSON.parse(text); } catch { /* leave null */ } }
        if (!r.ok) {
          throw new Error(
            body && body.message ? body.message :
            body && body.error === 'conflict' ? 'That code has been used already.' :
            body && body.error === 'not_found' ? 'No such code.' :
            'That did not work.');
        }
        stopScanning();
        scanBox.classList.remove('on');
        paste.value = '';
        await loadChannels();
      } catch (e) {
        scanMsg.textContent = e.message;
      } finally {
        pairing = false;
      }
    }

    async function decide(id, answer) {
      await fetch('/v1/pairings/' + id + '/decision', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ decision: answer }),
      });
      await loadChannels();
    }

    async function loadChannels() {
      let rows;
      try {
        const r = await fetch('/v1/pairings');
        if (!r.ok) throw new Error('http ' + r.status);
        rows = await r.json();
      } catch {
        chans.textContent = 'Could not load your channels.';
        return;
      }

      chans.textContent = '';
      if (!rows.length) {
        chans.appendChild(el('p', 'ch-empty',
          'None yet. They start by meeting somebody at a run.'));
        return;
      }

      rows.forEach(p => {
        const open = p.status === 'open' && p.peer_id;
        const row = el('button', 'ch-row');
        row.type = 'button';
        row.dataset.open = open ? '1' : '0';
        if (!open) row.disabled = true;

        // Before a channel opens there is no peer to name — a pairing
        // is a memory of meeting somebody, not a contact.
        row.appendChild(el('span', 'who',
          p.peer_name || (open ? 'Somebody from a run' : 'Not open yet')));
        row.appendChild(el('span', 'st', p.status));
        if (open) row.addEventListener('click', () => enter(p));
        chans.appendChild(row);

        // Three days later, both people are asked. A no is never
        // reported to the other side — see the pairing design.
        if (p.status === 'deciding' && !p.my_decision) {
          const decideRow = el('div', 'ch-decide');
          const yes = el('button', 'yes', 'yes');
          const nope = el('button', null, 'no');
          yes.type = nope.type = 'button';
          yes.addEventListener('click', () => decide(p.id, 'yes'));
          nope.addEventListener('click', () => decide(p.id, 'no'));
          decideRow.appendChild(yes);
          decideRow.appendChild(nope);
          chans.appendChild(decideRow);
        }
      });
    }

    async function enter(p) {
      current = p;
      room.classList.add('on');
      roomWho.textContent = 'With ' + (p.peer_name || 'somebody from a run');
      await draw();
    }

    async function draw() {
      list.textContent = 'Decrypting…';
      let msgs;
      try {
        msgs = await bfChat.read(current.id, current.peer_id);
      } catch (e) {
        list.textContent = e.message;
        return;
      }

      list.textContent = '';
      if (!msgs.length) {
        list.appendChild(el('p', 'ch-line', 'Nothing said yet.'));
        return;
      }

      msgs.forEach(m => {
        const mine = m.sender_id !== current.peer_id;
        const wrap = el('div', 'ch-msg' + (mine ? ' mine' : ''));
        wrap.appendChild(el('div', 'from',
          (mine ? 'you' : 'them') + ' · ' +
          new Date(m.at).toLocaleString(undefined,
            { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })));
        // Everything a person typed goes in as text, never as markup.
        wrap.appendChild(el('div', 'txt' + (m.text === null ? ' lost' : ''),
          m.text === null
            ? 'This one cannot be read — the key it was written for is gone.'
            : m.text));
        list.appendChild(wrap);
      });
    }

    sendBtn.addEventListener('click', async () => {
      const text = draft.value.trim();
      if (!current || !text) return;

      sendBtn.disabled = true;
      sendMsg.textContent = 'Encrypting…';
      try {
        await bfChat.send(current.id, current.peer_id, text);
        draft.value = '';
        sendMsg.textContent = '';
        await draw();
      } catch (e) {
        sendMsg.textContent = e.message;
      } finally {
        sendBtn.disabled = false;
      }
    });

    // Make and publish the key before anything needs it, so the first
    // message is not also the first round trip.
    bfChat.ready().catch(() => { /* surfaces when a channel is opened */ });
    loadChannels();
  };
})();
