// The running layer — sign up, log a run, be on the board.
//
// A runner is not a member. Anybody may become one with a username and
// a password, and nobody has verified them. A membership is the other
// thing: a number handed over in person at a run. The two are separate
// populations with separate credentials — see migration 0039 — and
// somebody may hold both.
//
// The distance is worked out here, in the browser, from the phone's own
// position fixes. **Only the total is sent.** No coordinates leave the
// device and there is no table on the other end to put them in. What
// the platform learns is how far and how long, which is what a board
// needs and nothing more.
//
// Which also explains why you hold the phone in your hand: a page only
// receives position updates while it is on screen. There is no
// background tracking in a browser, so the run lasts as long as the
// screen is up. A wake lock keeps it there.
(() => {
  'use strict';

  const cookie = (name) => {
    const hit = document.cookie.split(';')
      .map((p) => p.trim().split('='))
      .find(([k]) => k === name);
    return hit && hit[1] ? decodeURIComponent(hit[1]) : null;
  };

  // Script-readable on purpose: a username is the byline on the board,
  // not a credential. The credential is an HttpOnly cookie the page
  // never sees.
  const who = () => cookie('bf_runner');

  const el = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  };

  const clock = (s) => {
    const m = Math.floor(s / 60);
    return String(m).padStart(2, '0') + ':' + String(Math.floor(s % 60)).padStart(2, '0');
  };
  const km = (m) => (m / 1000).toFixed(2);
  // Minutes per kilometre — how runners actually talk about pace.
  const pace = (m, s) => {
    if (m < 50) return '—';
    const secPerKm = s / (m / 1000);
    return clock(secPerKm) + ' /km';
  };

  async function api(path, opts) {
    const r = await fetch('/v1/running' + path, opts);
    const raw = await r.text();
    let parsed = null;
    if (raw) { try { parsed = JSON.parse(raw); } catch { /* leave null */ } }
    if (!r.ok) throw new Error((parsed && (parsed.message || parsed.error)) || 'http ' + r.status);
    return parsed;
  }

  // ── Distance ──────────────────────────────────────────────────────
  // Haversine between consecutive fixes. Two filters, both necessary on
  // a phone: fixes with poor reported accuracy are thrown away, and so
  // is any single step longer than a sprinter could cover between
  // updates — that is the GPS jumping, not you.
  const R = 6371000;
  function metresBetween(a, b) {
    const toRad = (d) => (d * Math.PI) / 180;
    const dLat = toRad(b.lat - a.lat);
    const dLon = toRad(b.lon - a.lon);
    const s =
      Math.sin(dLat / 2) ** 2 +
      Math.cos(toRad(a.lat)) * Math.cos(toRad(b.lat)) * Math.sin(dLon / 2) ** 2;
    return 2 * R * Math.asin(Math.sqrt(s));
  }
  const WORST_ACCURACY = 35;
  const MAX_STEP = 80;

  window.renderRunning = function renderRunning(rootId) {
    const root = document.getElementById(rootId);
    if (!root) return;
    root.textContent = '';
    if (who()) mountTracker(root);
    else mountAuth(root);
  };

  // ── Signing up ────────────────────────────────────────────────────
  function mountAuth(root) {
    const box = el('div', 'r-auth');
    box.appendChild(el('p', 'r-lede',
      'Make a name and a password. Nothing else is asked for — no email, no phone.'));

    const user = el('input');
    user.type = 'text';
    user.placeholder = 'a name';
    user.autocomplete = 'username';
    user.maxLength = 20;

    const pass = el('input');
    pass.type = 'password';
    pass.placeholder = 'a password';
    pass.autocomplete = 'current-password';

    const go = el('button', 'r-go', 'Sign up');
    const alt = el('button', 'r-alt', 'I already have one');
    const msg = el('p', 'r-msg');

    let mode = 'signup';
    alt.addEventListener('click', () => {
      mode = mode === 'signup' ? 'login' : 'signup';
      go.textContent = mode === 'signup' ? 'Sign up' : 'Log in';
      alt.textContent = mode === 'signup' ? 'I already have one' : 'Make a new one';
      msg.textContent = '';
    });

    go.addEventListener('click', async () => {
      go.disabled = true;
      msg.className = 'r-msg';
      msg.textContent = 'One moment…';
      try {
        await api('/' + (mode === 'signup' ? 'signup' : 'login'), {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ username: user.value, password: pass.value }),
        });
        // The cookie is set by the server; re-render from it.
        renderRunning('run-panel');
        if (window.renderBoard) window.renderBoard('run-board');
      } catch (e) {
        msg.className = 'r-msg err';
        msg.textContent = e.message;
        go.disabled = false;
      }
    });

    [user, pass].forEach((i) => i.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') go.click();
    }));

    box.append(user, pass, go, alt, msg);
    root.appendChild(box);
  }

  // ── Running ───────────────────────────────────────────────────────
  function mountTracker(root) {
    const head = el('p', 'r-who');
    head.appendChild(el('span', null, who()));
    const out = el('button', 'r-out', 'log out');
    out.addEventListener('click', async () => {
      await fetch('/v1/running/session', { method: 'DELETE' }).catch(() => {});
      renderRunning('run-panel');
      if (window.renderBoard) window.renderBoard('run-board');
    });
    head.append(document.createTextNode(' · '), out);
    root.appendChild(head);

    const dial = el('div', 'r-dial');
    const dist = el('div', 'r-dist', '0.00');
    const unit = el('div', 'r-unit', 'km');
    const meta = el('div', 'r-meta', '00:00 · —');
    dial.append(dist, unit, meta);
    root.appendChild(dial);

    const go = el('button', 'r-go', 'Start a run');
    const msg = el('p', 'r-msg');
    root.append(go, msg);

    const mine = el('div', 'r-mine');
    root.appendChild(mine);
    loadMine(mine);

    let watchId = null;
    let lock = null;
    let last = null;
    let metres = 0;
    let startedAt = 0;
    let ticker = null;

    function paint() {
      const s = (Date.now() - startedAt) / 1000;
      dist.textContent = km(metres);
      meta.textContent = clock(s) + ' · ' + pace(metres, s);
    }

    async function start() {
      if (!navigator.geolocation) {
        msg.className = 'r-msg err';
        msg.textContent = 'This browser will not give a position.';
        return;
      }
      metres = 0;
      last = null;
      startedAt = Date.now();
      msg.className = 'r-msg';
      msg.textContent = 'Finding you… hold the phone and start moving.';

      // Keeps the screen on. Without it the page stops receiving fixes
      // the moment the display sleeps, which is the whole run.
      try { lock = await navigator.wakeLock.request('screen'); } catch { /* not everywhere */ }

      watchId = navigator.geolocation.watchPosition(
        (p) => {
          if (p.coords.accuracy > WORST_ACCURACY) return;
          const now = { lat: p.coords.latitude, lon: p.coords.longitude };
          if (last) {
            const step = metresBetween(last, now);
            if (step < MAX_STEP) metres += step;
          }
          last = now;
          if (msg.textContent) msg.textContent = '';
        },
        () => {
          msg.className = 'r-msg err';
          msg.textContent = 'No position. Allow location, and go outside.';
        },
        { enableHighAccuracy: true, maximumAge: 0, timeout: 20000 }
      );

      ticker = setInterval(paint, 500);
      go.textContent = 'Stop';
      go.classList.add('on');
      document.body.classList.add('running');
    }

    async function stop() {
      if (watchId !== null) navigator.geolocation.clearWatch(watchId);
      watchId = null;
      clearInterval(ticker);
      if (lock) { try { await lock.release(); } catch { /* fine */ } lock = null; }
      go.textContent = 'Start a run';
      go.classList.remove('on');
      document.body.classList.remove('running');

      const seconds = Math.round((Date.now() - startedAt) / 1000);
      const distance = Math.round(metres);
      if (distance <= 100 || seconds < 60) {
        msg.className = 'r-msg err';
        msg.textContent = 'Too short to log. A run is at least a minute and a hundred metres.';
        return;
      }
      msg.className = 'r-msg';
      msg.textContent = 'Logging…';
      try {
        await api('/runs', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ distance_m: distance, duration_s: seconds }),
        });
        msg.className = 'r-msg ok';
        msg.textContent = km(distance) + ' km in ' + clock(seconds) + '. Logged.';
        loadMine(mine);
        if (window.renderBoard) window.renderBoard('run-board');
      } catch (e) {
        msg.className = 'r-msg err';
        msg.textContent = e.message;
      }
    }

    go.addEventListener('click', () => (watchId === null ? start() : stop()));
  }

  async function loadMine(holder) {
    holder.textContent = '';
    let me;
    try { me = await api('/me'); } catch { return; }
    if (!me.runs.length) {
      holder.appendChild(el('p', 'r-none', 'Nothing logged yet.'));
      return;
    }
    const top = el('p', 'r-standing',
      me.runs.length + (me.runs.length === 1 ? ' run · ' : ' runs · ') +
      km(me.total_m) + ' km · score ' + me.score.toFixed(1));
    holder.appendChild(top);
    me.runs.slice(0, 8).forEach((r) => {
      const row = el('div', 'r-row');
      row.appendChild(el('span', 'r-rk', km(r.distance_m) + ' km'));
      row.appendChild(el('span', 'r-rt', clock(r.duration_s) + ' · ' + pace(r.distance_m, r.duration_s)));
      row.appendChild(el('span', 'r-rd',
        new Date(r.started_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })));
      holder.appendChild(row);
    });
  }

  // ── The board ─────────────────────────────────────────────────────
  window.renderBoard = async function renderBoard(rootId) {
    const root = document.getElementById(rootId);
    if (!root) return;
    let rows;
    try { rows = await api('/board'); } catch { root.textContent = 'The board could not be reached.'; return; }
    root.textContent = '';
    if (!rows.length) {
      root.appendChild(el('p', 'r-none', 'Nobody has logged a run yet. The first one is free.'));
      return;
    }
    rows.forEach((r, i) => {
      const row = el('div', 'r-brow');
      if (who() && r.username === who()) row.classList.add('me');
      row.appendChild(el('span', 'r-bn', String(i + 1)));
      row.appendChild(el('span', 'r-bu', r.username));
      row.appendChild(el('span', 'r-bd', km(r.total_m) + ' km · ' + r.runs));
      row.appendChild(el('span', 'r-bs', r.score.toFixed(1)));
      root.appendChild(row);
    });
  };
})();
