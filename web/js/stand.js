// Stand — a game where you win by making a road impossible to drive
// down.
//
// The inverse of Whyte. There you dodge cars, which is what the street
// does to you now. Here you end them, which is what the run club is
// arguing for.
//
// One input. A cursor sweeps the lanes; press and a person steps into
// the road wherever it is. Mistime it and a car takes them. Time it and
// they stay — and a person who stays becomes permanent. A body, then a
// bollard, then a table with somebody sitting at it. Each one narrows
// the road until there is no lane left, and the street stops being a
// road and becomes a place.
//
// The crowd is both the resource and the score: every drop spends
// somebody, every one who survives brings one more, because people
// gather where people are. Run out and the street stays a road.
//
// Call renderStand(elementId). It builds its own markup and styles, so
// a host supplies an empty div.
(() => {
  'use strict';

  const CSS = `
.bf-stand { --s-ink: var(--ink, #1a1a1a); --s-rule: var(--rule, #e6e6e6);
            --s-muted: var(--muted, #666); --s-accent: var(--accent, #b21b1b);
            --s-mono: 'IBM Plex Mono', ui-monospace, Menlo, monospace; }
.bf-stand canvas {
  display: block; width: 100%; height: auto; touch-action: none;
  background: #f7f6f3; border: 1px solid var(--s-rule); border-radius: 6px;
}
.bf-stand .s-hud {
  display: flex; justify-content: space-between; font-family: var(--s-mono);
  font-size: 11px; color: var(--s-muted); margin-top: 8px;
}
.bf-stand .s-note { font-size: 13px; color: var(--s-ink); margin: 8px 0 0; min-height: 19px; }
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

  window.renderStand = function renderStand(rootId) {
    const root = document.getElementById(rootId);
    if (!root) return;
    injectStyles();
    root.classList.add('bf-stand');
    root.textContent = '';

    const cv = el('canvas');
    cv.width = 420;
    cv.height = 208;
    root.appendChild(cv);

    const hud = el('div', 's-hud');
    const crowdEl = el('span', null, 'crowd 10');
    const streetEl = el('span', null, 'street 1');
    hud.appendChild(crowdEl);
    hud.appendChild(streetEl);
    root.appendChild(hud);

    const msg = el('p', 's-note', 'Space or tap. Step into the road when it is clear.');
    root.appendChild(msg);

    const ctx = cv.getContext('2d');
    const W = cv.width, H = cv.height;
    const INK = '#1a1a1a', ACCENT = '#b21b1b', MUTED = '#8b867d';
    const BEST_KEY = 'bf_stand_best';

    // Four lanes, because three is thin and five is a motorway.
    const LANES = 4;
    const TOP = 22, LANE_H = 40;
    const DROP_X = 168;
    // Three fixtures shut a lane: a person, a bollard, then a table.
    // Fewer and a street falls too fast to feel earned.
    const PER_LANE = 3;
    // How long somebody has to survive in the road before they are part
    // of it. Long enough that a lucky press is not enough.
    const HARDEN_MS = 620;

    let lanes, crowd, street, cursor, cursorDir, pending, running, over, best, last;

    function reset(full) {
      lanes = Array.from({ length: LANES }, () => ({ fixtures: [], cars: [], shut: false }));
      cursor = 0;
      cursorDir = 1;
      pending = null;
      over = false;
      if (full) {
        crowd = 10;
        street = 1;
      }
      try { best = parseInt(localStorage.getItem(BEST_KEY) || '0', 10) || 0; } catch { best = 0; }
      paintHud();
    }

    function paintHud() {
      crowdEl.textContent = 'crowd ' + crowd;
      streetEl.textContent = 'street ' + street + '   best ' + best;
    }

    // Cars come faster and closer together on every street. The floor
    // on the gap is what keeps a street possible rather than cruel.
    const carSpeed = () => 1.5 + street * 0.28;
    const carGap = () => Math.max(58, 150 - street * 9);

    function spawn(lane, i) {
      if (lane.shut) return;
      const lastCar = lane.cars[lane.cars.length - 1];
      const gap = carGap() + Math.random() * 90;
      if (!lastCar || lastCar.x < W - gap) {
        lane.cars.push({ x: W + 10, w: 34 + Math.random() * 16, stopped: 0 });
      }
    }

    function laneY(i) {
      return TOP + i * LANE_H;
    }

    function step(dt) {
      // The cursor sweeps. It is the only thing the player is really
      // reading, so it moves at a pace you can learn.
      cursor += cursorDir * dt * 0.055;
      if (cursor > LANES - 1) { cursor = LANES - 1; cursorDir = -1; }
      if (cursor < 0) { cursor = 0; cursorDir = 1; }

      lanes.forEach((lane, i) => {
        spawn(lane, i);
        lane.cars.forEach(c => {
          // A car brakes at the first thing standing in its lane and
          // then goes another way. The queue behind it is the point.
          const wall = lane.fixtures.reduce(
            (best, f) => (f.x > c.x - 200 && f.x < c.x ? Math.max(best, f.x) : best), -1);
          const blocker = lane.cars.find(o => o !== c && o.x < c.x && o.x > c.x - c.w - 14);
          if (wall > 0 && c.x - wall < 22) {
            c.stopped += dt * 16.7;
          } else if (blocker) {
            c.stopped += dt * 16.7;
          } else {
            c.x -= carSpeed() * dt;
          }
        });
        // Stopped long enough means it turned around.
        lane.cars = lane.cars.filter(c => c.stopped < 900 && c.x > -60);
      });

      if (pending) {
        pending.t += dt * 16.7;
        const lane = lanes[pending.lane];
        const hit = lane.cars.some(c => c.x < DROP_X + 5 && c.x + c.w > DROP_X - 5);
        if (hit) {
          pending = null;
          msg.textContent = 'Taken by a car.';
          if (crowd <= 0) return end();
        } else if (pending.t >= HARDEN_MS) {
          lane.fixtures.push({ x: DROP_X + (lane.fixtures.length - 1) * 26, kind: lane.fixtures.length });
          pending = null;
          crowd += 1;
          msg.textContent = 'They stayed.';
          if (lane.fixtures.length >= PER_LANE) {
            lane.shut = true;
            lane.cars = [];
          }
          if (lanes.every(l => l.shut)) return reclaimed();
          paintHud();
        }
      }
    }

    function reclaimed() {
      street += 1;
      if (street - 1 > best) {
        best = street - 1;
        try { localStorage.setItem(BEST_KEY, String(best)); } catch { /* private mode */ }
      }
      msg.textContent = 'The street is a place now. Next one.';
      reset(false);
      paintHud();
    }

    function end() {
      over = true;
      running = false;
      msg.textContent = 'The crowd ran out and it stayed a road. Space to start again.';
      paintHud();
    }

    function drop() {
      if (!running) {
        if (over) reset(true);
        running = true;
        msg.textContent = '';
        return;
      }
      if (pending) return;
      const lane = Math.round(cursor);
      if (lanes[lane].shut) { msg.textContent = 'That lane is already theirs.'; return; }
      crowd -= 1;
      paintHud();
      pending = { lane, t: 0 };
      if (crowd < 0) return end();
    }

    function draw() {
      ctx.clearRect(0, 0, W, H);

      lanes.forEach((lane, i) => {
        const y = laneY(i);
        // A shut lane is drawn as ground rather than road.
        ctx.fillStyle = lane.shut ? '#ece9e2' : 'transparent';
        if (lane.shut) ctx.fillRect(0, y - LANE_H + 12, W, LANE_H - 6);
        ctx.strokeStyle = lane.shut ? '#ddd8cf' : '#ddd8cf';
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, y + 0.5);
        ctx.lineTo(W, y + 0.5);
        ctx.stroke();

        lane.cars.forEach(c => {
          ctx.fillStyle = c.stopped > 0 ? '#d98b8b' : ACCENT;
          ctx.fillRect(c.x, y - 13, c.w, 13);
          ctx.fillStyle = '#f7f6f3';
          ctx.fillRect(c.x + 3, y - 10, 6, 4);
        });

        // What stayed. A body, then a bollard, then a table with
        // somebody at it — the street filling up with reasons to be on
        // it rather than to cross it.
        lane.fixtures.forEach(f => {
          ctx.fillStyle = INK;
          if (f.kind === 0) {
            ctx.fillRect(f.x - 2, y - 16, 4, 16);
            ctx.fillRect(f.x - 3, y - 22, 6, 5);
          } else if (f.kind === 1) {
            ctx.fillRect(f.x - 3, y - 12, 6, 12);
            ctx.fillStyle = ACCENT;
            ctx.fillRect(f.x - 3, y - 12, 6, 3);
          } else {
            ctx.fillRect(f.x - 9, y - 9, 18, 2);
            ctx.fillRect(f.x - 1, y - 9, 2, 9);
            ctx.fillRect(f.x - 15, y - 16, 4, 16);
            ctx.fillRect(f.x + 11, y - 16, 4, 16);
          }
        });
      });

      if (pending) {
        const y = laneY(pending.lane);
        const grown = pending.t / HARDEN_MS;
        ctx.fillStyle = INK;
        ctx.fillRect(DROP_X - 2, y - 16, 4, 16);
        ctx.fillRect(DROP_X - 3, y - 22, 6, 5);
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(DROP_X, y - 11, 13, -Math.PI / 2, -Math.PI / 2 + grown * Math.PI * 2);
        ctx.stroke();
      }

      if (running && !pending) {
        // The cursor: where somebody would step if you pressed now.
        const y = laneY(Math.round(cursor));
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1;
        ctx.setLineDash([3, 3]);
        ctx.beginPath();
        ctx.moveTo(DROP_X, y - 26);
        ctx.lineTo(DROP_X, y);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.fillStyle = ACCENT;
        ctx.fillRect(DROP_X - 4, y - 30, 8, 3);
      }

      if (!running) {
        ctx.fillStyle = 'rgba(247,246,243,0.88)';
        ctx.fillRect(0, H / 2 - 18, W, 36);
        ctx.fillStyle = over ? INK : MUTED;
        ctx.font = over
          ? 'italic 17px Newsreader, Georgia, serif'
          : '12px "IBM Plex Mono", ui-monospace, monospace';
        ctx.textAlign = 'center';
        ctx.fillText(over ? 'It stayed a road.' : 'tap or press space', W / 2, H / 2 + 5);
        ctx.textAlign = 'left';
      }
    }

    function tick(now) {
      const dt = Math.min(3, (now - (last || now)) / 16.67);
      last = now;
      if (running) step(dt);
      draw();
      requestAnimationFrame(tick);
    }

    function onScreen() {
      if (!root.offsetParent) return false;
      const w = root.closest('.win');
      return !w || (w.style.display !== 'none' && !w.hidden);
    }
    window.addEventListener('keydown', e => {
      if (e.code !== 'Space' && e.code !== 'ArrowUp') return;
      if (!onScreen()) return;
      e.preventDefault();
      drop();
    });
    cv.addEventListener('touchstart', e => { e.preventDefault(); drop(); }, { passive: false });
    cv.addEventListener('mousedown', drop);

    reset(true);
    running = false;
    requestAnimationFrame(tick);
  };
})();
