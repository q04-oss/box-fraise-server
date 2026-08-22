// The inbox, as a demonstration.
//
// Everything here is fake and nothing leaves the page. No account, no
// request to the server, no row anywhere. It exists so that somebody
// standing in a bakery with a phone can be shown what the thing is in
// about ten seconds, rather than told.
//
// The businesses are invented — the same three that are invented on
// /mvp — and the page says so. A demonstration that could be mistaken
// for a dollar somebody earned would be exactly the kind of thing this
// project is against.
//
// The real inbox is web/js/running.js and it talks to /v1/running/inbox.
// This file deliberately shares no code with it: a demo that drifts from
// the product is worse than no demo, but a demo wired into the product
// is a way to accidentally pay people for pressing a button on a poster.
(() => {
  'use strict';

  const ADS = [
    {
      who: 'Ferngrove Bakery',
      teaser: 'A loaf and a coffee, Saturday morning.',
      body: 'Ten percent off anything before nine on Saturdays. Say you ran here.',
    },
    {
      who: 'Norlight Coffee',
      teaser: 'The first one is on us, once.',
      body: 'Bring this in before noon and the first coffee is free. One each, and we will remember your face.',
    },
    {
      who: 'Hale & Co. Cycles',
      teaser: 'A tune-up while you run.',
      body: 'Drop the bike at eight and collect it at ten. Free for anybody who ran that morning.',
    },
  ];

  const CSS = `
.bf-demo {
  position: fixed; inset: 0; z-index: 90; display: none;
  background: #fff; overflow-y: auto;
  font-family: 'Newsreader', Georgia, serif;
}
.bf-demo.on { display: block; }
.bf-demo .wrap { max-width: 560px; margin: 0 auto; padding: 26px 22px 80px; }
.bf-demo .bar {
  display: flex; justify-content: space-between; align-items: baseline; gap: 16px;
  border-bottom: 1px solid #e6e6e6; padding-bottom: 12px; margin-bottom: 22px;
}
.bf-demo .lab {
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 10px; text-transform: uppercase; letter-spacing: 0.16em; color: #666;
}
.bf-demo .shut {
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 10px; text-transform: uppercase; letter-spacing: 0.16em;
  background: none; border: 1px solid #e6e6e6; border-radius: 999px;
  padding: 8px 14px; color: #666; cursor: pointer;
}
.bf-demo .shut:hover { color: #1a1a1a; border-color: #1a1a1a; }
.bf-demo h2 {
  font-style: italic; font-weight: 500; font-size: clamp(26px, 6vw, 38px);
  letter-spacing: -0.03em; line-height: 1.05; margin: 0 0 6px; color: #1a1a1a;
}
.bf-demo .bal {
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 11px; text-transform: uppercase; letter-spacing: 0.14em;
  color: #b21b1b; margin: 0 0 22px;
}
.bf-demo .list { border-top: 2px solid #1a1a1a; }
.bf-demo .card { border-bottom: 1px solid #e6e6e6; padding: 16px 0; }
.bf-demo .card.done { opacity: 0.4; }
.bf-demo .who { font-style: italic; font-size: 21px; line-height: 1.2; color: #1a1a1a; }
.bf-demo .tease { font-size: 16px; line-height: 1.5; color: #666; margin: 5px 0 12px; }
.bf-demo .btn {
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 11px; text-transform: uppercase; letter-spacing: 0.14em;
  padding: 13px 20px; border: 0; border-radius: 999px;
  background: #b21b1b; color: #fff; cursor: pointer; min-height: 46px;
}
.bf-demo .btn.no {
  background: none; color: #666; border: 1px solid #e6e6e6; margin-left: 8px;
}
.bf-demo .btn.no:hover { color: #1a1a1a; border-color: #1a1a1a; }
.bf-demo .btn[disabled] { background: #ccc; cursor: default; }
.bf-demo .said {
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 11px; text-transform: uppercase; letter-spacing: 0.13em; color: #666;
}
.bf-demo .fine { font-size: 14px; line-height: 1.6; color: #666; margin: 26px 0 0; }

/* Opening one. The advertisement takes the whole screen, which is what
   an advertisement would like to do everywhere and is only allowed to
   do here because somebody pressed a button. */
.bf-open {
  position: fixed; inset: 0; z-index: 95; display: none;
  background: #b21b1b; color: #fff;
  flex-direction: column; align-items: center; justify-content: center;
  text-align: center; padding: 8vw 7vw;
  font-family: 'Newsreader', Georgia, serif;
  animation: bfFlood 260ms cubic-bezier(0.2,0.8,0.2,1) both;
}
.bf-open.on { display: flex; }
@keyframes bfFlood { from { opacity: 0; transform: scale(1.03) } to { opacity: 1; transform: none } }
.bf-open .from {
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 11px; text-transform: uppercase; letter-spacing: 0.26em;
  color: rgba(255,255,255,0.75); margin-bottom: 20px;
}
.bf-open .said2 {
  font-style: italic; font-weight: 500;
  font-size: clamp(26px, 6.5vw, 52px); line-height: 1.1; letter-spacing: -0.02em;
  margin: 0; max-width: 20ch;
}
.bf-open .paid {
  font-style: italic; font-size: clamp(40px, 11vw, 76px); line-height: 1;
  margin-top: 40px;
}
.bf-open .paid small {
  display: block; font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 11px; text-transform: uppercase; letter-spacing: 0.2em;
  color: rgba(255,255,255,0.75); margin-top: 14px; font-style: normal;
}
.bf-open .on-btn {
  margin-top: 46px;
  font-family: 'IBM Plex Mono', ui-monospace, Menlo, monospace;
  font-size: 11px; text-transform: uppercase; letter-spacing: 0.16em;
  background: none; border: 1px solid rgba(255,255,255,0.6); border-radius: 999px;
  padding: 14px 26px; color: #fff; cursor: pointer;
}
.bf-open .on-btn:hover { background: #fff; color: #b21b1b; }
@media (prefers-reduced-motion: reduce) { .bf-open { animation: none } }
`;

  let built = false;
  let earned = 0;
  let done = 0;

  function el(t, c, x) {
    const n = document.createElement(t);
    if (c) n.className = c;
    if (x !== undefined) n.textContent = x;
    return n;
  }

  function build() {
    const style = document.createElement('style');
    style.textContent = CSS;
    document.head.appendChild(style);

    const sheet = el('div', 'bf-demo');
    sheet.id = 'bf-demo';
    const wrap = el('div', 'wrap');

    const bar = el('div', 'bar');
    bar.appendChild(el('span', 'lab', 'A demonstration'));
    const shut = el('button', 'shut', 'Close');
    bar.appendChild(shut);
    wrap.appendChild(bar);

    wrap.appendChild(el('h2', null, 'Your inbox.'));
    const bal = el('p', 'bal', 'Businesses ask. You decide.');
    wrap.appendChild(bal);

    const list = el('div', 'list');
    wrap.appendChild(list);

    // The whole point, said plainly. Anybody who presses these buttons
    // has not earned a dollar and should not think they have.
    wrap.appendChild(el('p', 'fine',
      'These three businesses are invented and no money changes hands here. '
      + 'In the real one you are paid a dollar for every advertisement you choose to open, '
      + 'and the advertiser pays three.'));

    const flood = el('div', 'bf-open');
    flood.id = 'bf-open';

    ADS.forEach((ad) => {
      const card = el('div', 'card');
      card.appendChild(el('div', 'who', ad.who));
      card.appendChild(el('div', 'tease', ad.teaser));

      const yes = el('button', 'btn', 'Open · $1.00');
      const no = el('button', 'btn no', 'No thanks');
      const acts = el('div');
      acts.append(yes, no);
      card.appendChild(acts);

      const settle = (word) => {
        card.classList.add('done');
        acts.remove();
        card.appendChild(el('div', 'said', word));
        done += 1;
        if (done === ADS.length) {
          bal.textContent = earned > 0
            ? 'That is $' + earned.toFixed(2) + ' you would have been paid'
            : 'You said no to all of it, and that cost you nothing';
        }
      };

      yes.addEventListener('click', () => {
        earned += 1;
        bal.textContent = '$' + earned.toFixed(2) + ' — for choosing to look';
        settle('Opened · $1.00');
        show(ad);
      });
      no.addEventListener('click', () => settle('Declined'));

      list.appendChild(card);
    });

    wrap.appendChild(el('div'));
    sheet.appendChild(wrap);
    document.body.appendChild(sheet);
    document.body.appendChild(flood);

    shut.addEventListener('click', close);
    document.addEventListener('keydown', (e) => {
      if (e.key !== 'Escape') return;
      if (flood.classList.contains('on')) flood.classList.remove('on');
      else if (sheet.classList.contains('on')) close();
    });
    built = true;
  }

  function show(ad) {
    const flood = document.getElementById('bf-open');
    flood.textContent = '';
    flood.appendChild(el('div', 'from', ad.who));
    flood.appendChild(el('p', 'said2', ad.body));

    const paid = el('div', 'paid', '+$1.00');
    paid.appendChild(el('small', null, 'paid to you for choosing to open it'));
    flood.appendChild(paid);

    const back = el('button', 'on-btn', 'Back to the inbox');
    back.addEventListener('click', () => flood.classList.remove('on'));
    flood.appendChild(back);

    flood.classList.add('on');
  }

  function close() {
    document.getElementById('bf-demo').classList.remove('on');
    document.getElementById('bf-open').classList.remove('on');
    document.body.style.overflow = '';
  }

  window.openInboxDemo = function openInboxDemo() {
    if (!built) build();
    document.getElementById('bf-demo').classList.add('on');
    document.body.style.overflow = 'hidden';
  };
})();
