// Whyte — an endless runner where the obstacles are cars.
//
// That is the argument rather than a theme: the street is organised
// around moving vehicles through it, and the person on foot is the one
// who gets out of the way. Run into one and the game says so.
//
// Shared by /whyte, where it is the page, and by the Whyte window on
// /os, where it is an app on a mock desktop. One implementation: the
// physics and the leaderboard have to agree, and two copies would
// drift.
//
// Call renderWhyte(elementId). It builds its own markup and styles into
// whatever container it is given, so a host supplies an empty div.
//
// Nothing here is scored against the platform and nothing is tracked.
// The leaderboard is three letters and a distance — deliberately
// outside the member system, because applying it to a game about
// jumping over cars would be taking a toy seriously. See 0035.
(() => {
  'use strict';

  const CSS = `
.bf-whyte { --w-ink: var(--ink, #1a1a1a); --w-rule: var(--rule, #e6e6e6);
            --w-muted: var(--muted, #666); --w-accent: var(--accent, #b21b1b);
            --w-mono: 'IBM Plex Mono', ui-monospace, Menlo, monospace; }
.bf-whyte canvas {
  display: block; width: 100%; height: auto; touch-action: none;
  background: #f7f6f3; border: 1px solid var(--w-rule); border-radius: 6px;
}
.bf-whyte .w-hud {
  display: flex; justify-content: space-between; font-family: var(--w-mono);
  font-size: 11px; color: var(--w-muted); margin-top: 8px;
}
.bf-whyte .w-note { font-size: 13px; color: var(--w-ink); margin: 8px 0 0; min-height: 19px; }
.bf-whyte .w-enter { display: flex; align-items: center; gap: 8px; margin-top: 10px; }
.bf-whyte .w-enter[hidden] { display: none; }
.bf-whyte .w-enter label {
  font-family: var(--w-mono); font-size: 10px; text-transform: uppercase;
  letter-spacing: 0.12em; color: var(--w-muted);
}
.bf-whyte .w-enter input {
  width: 66px; font-family: var(--w-mono); font-size: 15px; letter-spacing: 0.18em;
  text-transform: uppercase; text-align: center; padding: 6px;
  border: 1px solid var(--w-rule); border-radius: 5px; background: #fff; color: var(--w-ink);
}
.bf-whyte .w-enter input:focus { outline: none; border-color: var(--w-accent); }
.bf-whyte .w-enter button {
  font-family: var(--w-mono); font-size: 10px; text-transform: uppercase;
  letter-spacing: 0.12em; padding: 7px 13px; border: 1px solid var(--w-ink);
  border-radius: 999px; background: none; color: var(--w-ink); cursor: pointer;
}
.bf-whyte .w-enter button:hover { border-color: var(--w-accent); color: var(--w-accent); }
.bf-whyte .w-board { list-style: none; margin: 10px 0 0; padding: 0;
  font-family: var(--w-mono); font-size: 11px; }
.bf-whyte .w-board li {
  display: flex; gap: 10px; padding: 4px 0;
  border-bottom: 1px solid var(--w-rule); color: var(--w-muted);
}
.bf-whyte .w-board li:last-child { border-bottom: 0; }
.bf-whyte .w-board .pos { width: 20px; flex: 0 0 auto; }
.bf-whyte .w-board .who { flex: 1; color: var(--w-ink); letter-spacing: 0.14em; }
.bf-whyte .w-board .far { flex: 0 0 auto; font-variant-numeric: tabular-nums; }
`;

  let styled = false;
  function injectStyles() {
    if (styled) return;
    styled = true;
    const s = document.createElement('style');
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  }

  window.renderWhyte = function renderWhyte(rootId) {
    const root = document.getElementById(rootId);
    if (!root) return;
    injectStyles();
    root.classList.add('bf-whyte');
    root.textContent = '';

    const cv = el('canvas');
    cv.width = 420;
    cv.height = 180;
    root.appendChild(cv);

    const hud = el('div', 'w-hud');
    const scoreEl = el('span', null, '0 m');
    const bestEl = el('span', null, 'best 0 m');
    hud.appendChild(scoreEl);
    hud.appendChild(bestEl);
    root.appendChild(hud);

    const msg = el('p', 'w-note', 'Space or tap to jump. Hold it to jump further.');
    root.appendChild(msg);

    const enter = el('div', 'w-enter');
    enter.hidden = true;
    const label = el('label', null, 'Initials');
    label.setAttribute('for', 'w-initials');
    const initials = el('input');
    initials.id = 'w-initials';
    initials.maxLength = 3;
    initials.autocomplete = 'off';
    initials.spellcheck = false;
    const post = el('button', null, 'Put it up');
    post.type = 'button';
    enter.appendChild(label);
    enter.appendChild(initials);
    enter.appendChild(post);
    root.appendChild(enter);

    const boardEl = el('ol', 'w-board');
    root.appendChild(boardEl);

    // ── The game ─────────────────────────────────────────────────
    const ctx = cv.getContext('2d');
    const W = cv.width, H = cv.height;
    const GROUND = H - 34;
    const INK = '#1a1a1a', ACCENT = '#b21b1b', MUTED = '#8b867d';
    const BEST_KEY = 'bf_whyte_best', WHO_KEY = 'bf_whyte_initials';

    const runner = { x: 46, y: GROUND, vy: 0, w: 9, h: 20 };
    // Tuned against the obstacles rather than by feel: a tap clears a
    // short car with room, a held jump clears a long one, and airtime
    // is shorter than the smallest gap so you never come down on the
    // car behind the one you jumped.
    const GRAVITY = 0.9, JUMP = -7.8, HOLD = -0.22, HOLD_MS = 160;

    let cars, speed, dist, best, running, over, held, heldFor, lamps, last;
    let lastRun = 0, minToPost = 0;

    try { initials.value = localStorage.getItem(WHO_KEY) || ''; } catch { /* private mode */ }

    function reset() {
      cars = [];
      lamps = [120, 300, 480];
      speed = 3.1;
      dist = 0;
      over = false;
      held = false;
      heldFor = 0;
      runner.y = GROUND;
      runner.vy = 0;
      try { best = parseInt(localStorage.getItem(BEST_KEY) || '0', 10) || 0; } catch { best = 0; }
      paintHud();
    }

    function paintHud() {
      scoreEl.textContent = Math.floor(dist) + ' m';
      bestEl.textContent = 'best ' + best + ' m';
    }

    function spawn() {
      const lastCar = cars[cars.length - 1];
      const gap = 150 + Math.random() * 130 - Math.min(40, speed * 5);
      if (!lastCar || W - lastCar.x > gap) {
        const long = Math.random() < 0.28;
        cars.push({ x: W + 20, w: long ? 46 : 30, h: long ? 15 : 13 });
      }
    }

    function step(dt) {
      dist += speed * dt * 0.34;
      speed = 3.1 + Math.min(3.6, dist / 260);

      if (held && heldFor < HOLD_MS && runner.vy < 0) {
        runner.vy += HOLD;
        heldFor += dt * 16.7;
      }
      runner.vy += GRAVITY;
      runner.y += runner.vy;
      if (runner.y > GROUND) { runner.y = GROUND; runner.vy = 0; }

      spawn();
      cars.forEach(c => { c.x -= speed * dt; });
      cars = cars.filter(c => c.x + c.w > -10);
      lamps = lamps.map(x => (x - speed * dt * 0.45 < -20 ? W + 40 : x - speed * dt * 0.45));

      const rx = runner.x + 1, rw = runner.w - 2;
      for (const c of cars) {
        if (rx < c.x + c.w - 2 && rx + rw > c.x + 2 && runner.y > GROUND - c.h) return end();
      }
    }

    function end() {
      over = true;
      running = false;
      const m = Math.floor(dist);
      lastRun = m;
      if (m > best) {
        best = m;
        try { localStorage.setItem(BEST_KEY, String(best)); } catch { /* private mode */ }
        msg.textContent = 'Furthest yet: ' + m + ' m.';
      } else {
        msg.textContent = 'Stopped by a car at ' + m + ' m.';
      }
      paintHud();
      offerBoard(m);
    }

    function draw() {
      ctx.clearRect(0, 0, W, H);
      ctx.strokeStyle = INK;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, GROUND + 0.5);
      ctx.lineTo(W, GROUND + 0.5);
      ctx.stroke();

      ctx.fillStyle = '#ddd8cf';
      lamps.forEach(x => ctx.fillRect(x, GROUND - 30, 2, 30));

      ctx.fillStyle = INK;
      const airborne = runner.y < GROUND;
      ctx.fillRect(runner.x, runner.y - runner.h, runner.w, runner.h - 6);
      const swing = airborne ? 3 : (Math.floor(dist * 2.2) % 2 ? 3 : -3);
      ctx.fillRect(runner.x + 1, runner.y - 6, 3, 6);
      ctx.fillRect(runner.x + runner.w - 4 + swing * 0.4, runner.y - 6, 3, 6);

      cars.forEach(c => {
        ctx.fillStyle = ACCENT;
        ctx.fillRect(c.x, GROUND - c.h, c.w, c.h);
        ctx.fillStyle = '#f7f6f3';
        ctx.fillRect(c.x + c.w - 9, GROUND - c.h + 3, 6, 4);
        ctx.fillStyle = INK;
        ctx.fillRect(c.x + 5, GROUND - 2, 5, 3);
        ctx.fillRect(c.x + c.w - 11, GROUND - 2, 5, 3);
      });

      if (over) {
        ctx.fillStyle = 'rgba(247,246,243,0.86)';
        ctx.fillRect(0, GROUND / 2 - 16, W, 34);
        ctx.fillStyle = INK;
        ctx.font = 'italic 17px Newsreader, Georgia, serif';
        ctx.textAlign = 'center';
        ctx.fillText('The street was organised around them.', W / 2, GROUND / 2 + 6);
        ctx.textAlign = 'left';
      }
      if (!running && !over) {
        ctx.fillStyle = MUTED;
        ctx.font = '12px "IBM Plex Mono", ui-monospace, monospace';
        ctx.textAlign = 'center';
        ctx.fillText('tap or press space', W / 2, GROUND / 2 + 4);
        ctx.textAlign = 'left';
      }
    }

    function frame(now) {
      const dt = Math.min(3, (now - (last || now)) / 16.67);
      last = now;
      if (running) { step(dt); paintHud(); }
      draw();
      requestAnimationFrame(frame);
    }

    function jump() {
      if (!running) {
        if (over) reset();
        running = true;
        msg.textContent = '';
        return;
      }
      if (runner.y >= GROUND) {
        runner.vy = JUMP;
        held = true;
        heldFor = 0;
      }
    }

    // ── The board ────────────────────────────────────────────────
    function drawBoard(rows) {
      boardEl.textContent = '';
      rows.forEach((r, i) => {
        const li = el('li');
        li.appendChild(el('span', 'pos', (i + 1) + '.'));
        li.appendChild(el('span', 'who', r.initials));
        li.appendChild(el('span', 'far', r.metres + ' m'));
        boardEl.appendChild(li);
      });
      minToPost = rows.length >= 10 ? rows[rows.length - 1].metres : 0;
    }

    async function loadBoard() {
      try {
        const r = await fetch('/v1/whyte/scores', { cache: 'no-store' });
        if (r.ok) drawBoard(await r.json());
      } catch { /* a board that will not load is not worth a message */ }
    }

    function offerBoard(m) {
      if (m <= minToPost) return;
      enter.hidden = false;
      const known = /^[A-Z]{3}$/.test(initials.value.trim().toUpperCase());
      post.textContent = known
        ? 'Put it up as ' + initials.value.trim().toUpperCase()
        : 'Put it up';
      (known ? post : initials).focus();
    }

    post.addEventListener('click', async () => {
      const who = initials.value.trim().toUpperCase();
      if (!/^[A-Z]{3}$/.test(who)) { msg.textContent = 'Three letters, A to Z.'; initials.focus(); return; }
      try {
        const r = await fetch('/v1/whyte/scores', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ initials: who, metres: lastRun }),
        });
        const body = await r.json();
        if (!r.ok) throw new Error(body.message || 'it did not save');
        enter.hidden = true;
        try { localStorage.setItem(WHO_KEY, who); } catch { /* private mode */ }
        msg.textContent = body.rank ? 'Number ' + body.rank + '.' : 'Up there somewhere.';
        await loadBoard();
      } catch (e) {
        msg.textContent = e.message;
      }
    });

    initials.addEventListener('keydown', e => {
      if (e.key === 'Enter') { e.preventDefault(); post.click(); }
      e.stopPropagation();
    });

    // ── Input ────────────────────────────────────────────────────
    // Space only when this game is the thing on screen — on /os the
    // desktop has other windows, and a jump fired at somebody reading
    // Permissions is a bug.
    function onScreen() {
      if (!root.offsetParent) return false;
      const w = root.closest('.win');
      return !w || (w.style.display !== 'none' && !w.hidden);
    }
    window.addEventListener('keydown', e => {
      if (e.code !== 'Space' && e.code !== 'ArrowUp') return;
      if (!onScreen()) return;
      if (document.activeElement === initials || document.activeElement === post) return;
      e.preventDefault();
      jump();
    });
    window.addEventListener('keyup', e => {
      if (e.code === 'Space' || e.code === 'ArrowUp') held = false;
    });

    // Touch, because most links are opened on a phone and a game that
    // cannot be played on one is not a game anybody passes on.
    cv.addEventListener('touchstart', e => { e.preventDefault(); jump(); }, { passive: false });
    cv.addEventListener('touchend', () => { held = false; });
    cv.addEventListener('mousedown', jump);
    window.addEventListener('mouseup', () => { held = false; });

    reset();
    running = false;
    loadBoard();
    requestAnimationFrame(frame);
  };
})();
