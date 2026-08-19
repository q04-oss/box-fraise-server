// The calendar.
//
// A member should not have to chase their own schedule. A business on
// the platform publishes it and what it publishes is what is true —
// no group chat, no text at eleven at night, no on-call ambiguity.
//
// Shifts and runs sit in one list because they compete for the same
// hours. Somebody deciding whether they can make Sunday at eight needs
// to see the Saturday night close on the same page, and a business
// scheduling over the run becomes a visible choice rather than an
// accident.
//
// Call renderCalendar(elementId). Same contract as channels.js: it
// builds its own markup and styles into whatever container it is
// given, so a host supplies an empty div and loads session.js first.
(() => {
  'use strict';

  const CSS = `
.bf-cal { --cal-ink: var(--ink, #1a1a1a); --cal-rule: var(--rule, #e6e6e6);
          --cal-muted: var(--muted, #666); --cal-accent: var(--accent, #b21b1b);
          --cal-mono: 'IBM Plex Mono', ui-monospace, Menlo, monospace; }
.bf-cal .cal-day {
  font-family: var(--cal-mono); font-size: 10px; text-transform: uppercase;
  letter-spacing: 0.14em; color: var(--cal-muted);
  border-top: 1px solid var(--cal-rule); padding: 18px 0 8px; margin: 0;
}
.bf-cal .cal-day:first-child { border-top: 0; padding-top: 0; }
.bf-cal .cal-row { display: flex; gap: 14px; align-items: baseline; padding: 7px 0; }
.bf-cal .cal-time {
  font-family: var(--cal-mono); font-size: 12px; color: var(--cal-ink);
  flex: 0 0 auto; min-width: 92px; font-variant-numeric: tabular-nums;
}
.bf-cal .cal-what { flex: 1; font-size: 16px; line-height: 1.35; }
.bf-cal .cal-tag {
  font-family: var(--cal-mono); font-size: 9px; text-transform: uppercase;
  letter-spacing: 0.12em; flex: 0 0 auto; color: var(--cal-accent);
}
.bf-cal .cal-row.run .cal-what { font-style: italic; }
.bf-cal .cal-row.off .cal-what { text-decoration: line-through; color: var(--cal-muted); }
.bf-cal .cal-row.off .cal-time { color: var(--cal-muted); }
.bf-cal .cal-row.off .cal-tag { color: var(--cal-muted); }
.bf-cal .cal-empty { font-size: 16px; color: var(--cal-muted); margin: 0; }
.bf-cal .cal-note { font-size: 13px; line-height: 1.6; color: var(--cal-muted); margin-top: 26px; }
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

  const dayLabel = d => d.toLocaleDateString(undefined,
    { weekday: 'long', month: 'short', day: 'numeric' });
  const timeLabel = d => d.toLocaleTimeString(undefined,
    { hour: '2-digit', minute: '2-digit' });

  window.renderCalendar = async function renderCalendar(rootId) {
    const root = document.getElementById(rootId);
    if (!root || !window.bfSession || !bfSession.signedIn()) return;
    injectStyles();
    root.classList.add('bf-cal');
    root.textContent = 'Loading…';

    let entries;
    try {
      const r = await fetch('/v1/members/calendar');
      if (!r.ok) throw new Error('http ' + r.status);
      entries = await r.json();
    } catch {
      root.textContent = 'The calendar could not be reached.';
      return;
    }

    root.textContent = '';
    if (!entries.length) {
      root.appendChild(el('p', 'cal-empty',
        'Nothing on it yet. Shifts appear here when a business publishes them.'));
      return;
    }

    // Grouped by day, because that is how somebody reads a week.
    let lastDay = null;
    entries.forEach(e => {
      const start = new Date(e.starts_at);
      const day = start.toDateString();
      if (day !== lastDay) {
        lastDay = day;
        root.appendChild(el('p', 'cal-day', dayLabel(start)));
      }

      const row = el('div', 'cal-row' + (e.kind === 'run' ? ' run' : '') +
                              (e.cancelled_at ? ' off' : ''));
      const span = e.ends_at
        ? timeLabel(start) + '–' + timeLabel(new Date(e.ends_at))
        : timeLabel(start);
      row.appendChild(el('span', 'cal-time', span));
      // Everything here is a name somebody typed. textContent, always.
      row.appendChild(el('span', 'cal-what', e.what));
      row.appendChild(el('span', 'cal-tag',
        e.cancelled_at ? 'cancelled' : (e.kind === 'run' ? 'run' : 'shift')));
      root.appendChild(row);
    });

    root.appendChild(el('p', 'cal-note',
      'A published shift is not a suggestion. It can be cancelled — and you will see that ' +
      'it was — but it cannot be quietly moved to a different time.'));
  };
})();
