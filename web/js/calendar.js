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
.bf-cal .cal-head {
  border-left: 2px solid var(--cal-accent); padding: 2px 0 2px 16px; margin: 0 0 26px;
}
.bf-cal .cal-head-kv {
  font-family: var(--cal-mono); font-size: 10px; text-transform: uppercase;
  letter-spacing: 0.14em; color: var(--cal-muted); margin: 0 0 6px;
}
.bf-cal .cal-head-when { font-size: 18px; margin: 0 0 3px; }
.bf-cal .cal-head-where { font-size: 14px; color: var(--cal-muted); margin: 0; }
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

  // The run club's standing schedule. Fixed, known, and true whether or
  // not anybody has created an event row for a particular week — a
  // calendar that is empty until an admin fills it in is not a
  // calendar. Mirrors /runclub, which is where the argument for the
  // times lives.
  const RUN_CLUB = {
    where: 'Dr. Wilbert McIntyre Park, Old Strathcona',
    // 5 = Friday, 0 = Sunday.
    days: [
      { day: 5, hour: 18, minute: 0, what: 'Run club — out of the week' },
      { day: 0, hour: 8,  minute: 0, what: 'Run club — back into it' },
    ],
  };
  const RUN_WEEKS = 3;

  /// The next few Fridays at six and Sundays at eight.
  ///
  /// Generated rather than stored, because they are a rule and not a
  /// list. Any that collide with a real event row are dropped, so an
  /// actual run somebody created wins over the standing one.
  function standingRuns(from, taken) {
    const out = [];
    for (let i = 0; i < RUN_WEEKS * 7; i++) {
      const d = new Date(from);
      d.setDate(d.getDate() + i);
      RUN_CLUB.days.forEach(slot => {
        if (d.getDay() !== slot.day) return;
        const at = new Date(d);
        at.setHours(slot.hour, slot.minute, 0, 0);
        if (at < from) return;
        if (taken.has(at.toDateString() + slot.hour)) return;
        out.push({
          kind: 'run',
          id: 'standing-' + at.toISOString(),
          what: slot.what,
          starts_at: at.toISOString(),
          ends_at: null,
          cancelled_at: null,
          standing: true,
        });
      });
    }
    return out;
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

    // The runs the club holds every week, folded in with whatever the
    // server returned, so the calendar is never empty and never has to
    // be told when Friday is.
    const taken = new Set(entries
      .filter(e => e.kind === 'run')
      .map(e => { const d = new Date(e.starts_at); return d.toDateString() + d.getHours(); }));
    entries = entries
      .concat(standingRuns(new Date(), taken))
      .sort((a, b) => new Date(a.starts_at) - new Date(b.starts_at));

    root.textContent = '';

    // Where the run club is, always, above the week itself.
    const head = el('div', 'cal-head');
    head.appendChild(el('p', 'cal-head-kv', 'The run club'));
    head.appendChild(el('p', 'cal-head-when', 'Friday at six · Sunday at eight'));
    head.appendChild(el('p', 'cal-head-where', RUN_CLUB.where));
    root.appendChild(head);

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
        e.cancelled_at ? 'cancelled' : (e.kind === 'run' ? 'run club' : 'shift')));
      root.appendChild(row);
    });

    root.appendChild(el('p', 'cal-note',
      'A published shift is not a suggestion. It can be cancelled — and you will see that ' +
      'it was — but it cannot be quietly moved to a different time.'));
  };
})();
