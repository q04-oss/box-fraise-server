// The inbox — businesses ask, you decide, you are paid when you say yes.
//
// The first phone drawn on /mvp, made real. Everything about how it
// behaves is an argument, so the arguments are written down here rather
// than left to be inferred:
//
//   The list is not ranked and not addressed. Every current member is
//   looking at the same offers in the same order. There is no scoring
//   function in this file and there is not meant to be one.
//
//   Nothing measures whether you looked. No dwell timer, no scroll
//   depth, no beacon. Pressing the button is the whole transaction.
//
//   Saying yes cannot be undone and does not fire twice: the button is
//   disabled on the way to the server, and the server refuses a second
//   receipt for the same offer anyway.
//
// Built into whatever container it is given, so a host supplies an empty
// <div>. Styles are injected once and scoped under .bf-in.
//
// No token is read anywhere below. The credential is an HttpOnly cookie
// and same-origin fetch sends it unasked — see web/js/session.js.
(() => {
  'use strict';

  const CSS = `
.bf-in { --in-ink: var(--ink, #1a1a1a); --in-rule: var(--rule, #e6e6e6);
         --in-muted: var(--muted, #666); --in-accent: var(--accent, #b21b1b);
         --in-mono: 'IBM Plex Mono', ui-monospace, Menlo, monospace; }
.bf-in .in-bar {
  display: flex; justify-content: space-between; align-items: baseline; gap: 16px;
  border-bottom: 1px solid var(--in-rule); padding-bottom: 10px; margin-bottom: 18px;
  font-family: var(--in-mono); font-size: 11px; text-transform: uppercase;
  letter-spacing: 0.14em; color: var(--in-muted);
}
.bf-in .in-bal { color: var(--in-ink); }
.bf-in .in-bal b { color: var(--in-accent); font-weight: 500; }
.bf-in .in-card {
  display: flex; gap: 13px; align-items: center;
  border: 1px solid var(--in-rule); border-radius: 10px;
  padding: 13px 12px; margin-bottom: 10px; background: #fff;
  transition: border-color 120ms ease, background 120ms ease;
}
.bf-in .in-card.took { border-color: var(--in-accent); background: #fdf2f2; }
.bf-in .in-mark {
  width: 42px; height: 42px; border-radius: 7px; flex: 0 0 auto;
  background: #f0f0f0; object-fit: cover;
}
.bf-in .in-who { flex: 1 1 auto; min-width: 0; }
.bf-in .in-name { font-size: 16px; line-height: 1.25; }
.bf-in .in-what {
  font-family: var(--in-mono); font-size: 9px; text-transform: uppercase;
  letter-spacing: 0.13em; color: var(--in-muted); margin-top: 4px;
}
.bf-in .in-head { font-size: 15px; line-height: 1.45; margin: 6px 0 0; color: var(--in-ink); }
.bf-in .in-yes {
  font-family: var(--in-mono); font-size: 11px; text-transform: uppercase;
  letter-spacing: 0.13em; padding: 12px 16px; border: 0; border-radius: 999px;
  background: var(--in-accent); color: #fff; cursor: pointer; flex: 0 0 auto;
  min-height: 44px;
}
.bf-in .in-yes[disabled] { background: #ccc; cursor: default; }
.bf-in .in-paid {
  font-family: 'Newsreader', Georgia, serif; font-style: italic;
  font-size: 26px; color: var(--in-accent); flex: 0 0 auto;
}
.bf-in .in-note { font-size: 14px; color: var(--in-muted); margin: 10px 0 0; }
.bf-in .in-empty { font-size: 16px; color: var(--in-muted); margin: 0; }
`;

  let styled = false;
  function injectStyles() {
    if (styled) return;
    styled = true;
    const s = document.createElement('style');
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  const money = (cents) => '$' + (cents / 100).toFixed(2);

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  }

  window.renderInbox = async function renderInbox(rootId) {
    const root = document.getElementById(rootId);
    if (!root) return;
    injectStyles();
    root.classList.add('bf-in');
    root.textContent = 'Looking…';

    let data;
    try {
      const r = await fetch('/v1/inbox');
      if (!r.ok) throw new Error('http ' + r.status);
      data = await r.json();
    } catch {
      root.textContent = 'The inbox could not be reached.';
      return;
    }

    root.textContent = '';

    const bar = el('div', 'in-bar');
    bar.appendChild(el('span', null, 'Inbox'));
    const bal = el('span', 'in-bal');
    root.appendChild(bar);
    bar.appendChild(bal);

    // Owed is the live number; collected is the one that says any of
    // this ever turned into money in a hand.
    function paintBalance(owed) {
      bal.textContent = '';
      const b = el('b', null, money(owed));
      bal.appendChild(document.createTextNode('waiting for you  '));
      bal.appendChild(b);
      if (data.paid_cents > 0) {
        bal.appendChild(document.createTextNode('  ·  collected ' + money(data.paid_cents)));
      }
    }
    paintBalance(data.owed_cents);

    if (!data.offers.length) {
      const p = el('p', 'in-empty',
        'Nothing is waiting. When a business buys space it turns up here, and you decide.');
      root.appendChild(p);
      if (data.owed_cents > 0) {
        root.appendChild(el('p', 'in-note',
          'You are owed ' + money(data.owed_cents) + '. Collect it in cash at a run.'));
      }
      return;
    }

    data.offers.forEach((offer) => {
      const card = el('div', 'in-card');

      const img = document.createElement('img');
      img.className = 'in-mark';
      img.loading = 'lazy';
      img.alt = '';
      img.src = '/v1/marks/' + offer.mark_id + '/image';
      card.appendChild(img);

      const who = el('div', 'in-who');
      // A business name when one bought it; the mark's own label when
      // it belongs to the platform.
      who.appendChild(el('div', 'in-name', offer.business_name || offer.label));
      who.appendChild(el('div', 'in-what',
        'Wants to advertise · ' + money(offer.amount_cents)));
      // Arbitrary text an admin typed. textContent, like everywhere else.
      who.appendChild(el('p', 'in-head', offer.headline));
      card.appendChild(who);

      const yes = el('button', 'in-yes', 'Yes');
      yes.type = 'button';
      card.appendChild(yes);

      yes.addEventListener('click', async () => {
        yes.disabled = true;
        try {
          const r = await fetch('/v1/inbox/' + offer.id + '/accept', { method: 'POST' });
          const raw = await r.text();
          let parsed = null;
          if (raw) { try { parsed = JSON.parse(raw); } catch { /* leave null */ } }
          if (!r.ok) {
            throw new Error((parsed && (parsed.message || parsed.error)) || 'http ' + r.status);
          }

          // The payout, in place of the button. /mvp draws this as a
          // sheet; on a page inside a feed it belongs on the card that
          // caused it.
          card.classList.add('took');
          yes.remove();
          card.appendChild(el('div', 'in-paid', '+' + money(parsed.amount_cents)));
          who.querySelector('.in-what').textContent = 'Paid to you for choosing to watch';
          paintBalance(parsed.owed_cents);
        } catch (e) {
          yes.disabled = false;
          const note = card.parentNode.querySelector('.in-err') || el('p', 'in-note in-err');
          note.textContent = e.message || 'That did not go through.';
          root.appendChild(note);
        }
      });

      root.appendChild(card);
    });

    root.appendChild(el('p', 'in-note',
      'What you are owed is collected in cash, in person, at a run.'));
  };
})();
