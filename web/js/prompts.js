// The three things there are to answer.
//
// One list, because there were four copies of it: gurgle.js, the admin
// tool, Prompt::ALL in Rust, and the magazine page. The keys are the
// wire values and they must match Prompt::ALL in
// src/domain/submissions/types.rs and the CHECK constraints in 0028 and
// 0034 — a change here without a migration is a 400.
//
// The admin tool keeps its own copy on purpose: it ships to a different
// place than the site does, and a shared file it cannot reach is worse
// than a duplicate it can.
//
// "for better taste" is deliberately not explained anywhere. A gloss
// would kill it.
(() => {
  'use strict';

  const PROMPTS = [
    { key: 'run_country',  label: 'Do I Have What it Takes To Run This Country?' },
    { key: 'run_away',     label: 'Why Do I Want To Run Away?' },
    { key: 'better_taste', label: 'for better taste…' },
  ];

  window.bfPrompts = PROMPTS;
  window.bfPromptLabel = key =>
    (PROMPTS.find(p => p.key === key) || {}).label || '';

  // Styles for the picker, injected once so no host carries a copy.
  let styled = false;
  window.bfPromptStyles = function bfPromptStyles() {
    if (styled) return;
    styled = true;
    const el = document.createElement('style');
    el.textContent =
      '.gg-prompts{display:flex;flex-direction:column;gap:8px;margin:0 0 18px}' +
      '.gg-prompts button{font:inherit;text-align:left;cursor:pointer;' +
      'border:1px solid var(--rule,#e6e6e6);background:none;color:inherit;' +
      'border-radius:10px;padding:12px 14px;line-height:1.35;' +
      'transition:border-color 120ms ease,color 120ms ease}' +
      '.gg-prompts button:hover{border-color:var(--ink,#1a1a1a)}' +
      '.gg-prompts button[aria-pressed="true"]{border-color:var(--accent,#b21b1b);' +
      'color:var(--accent,#b21b1b)}';
    document.head.appendChild(el);
  };

  /// Build a picker into `host`. Returns a function giving the chosen
  /// key, so a caller keeps no state of its own.
  window.bfPromptPicker = function bfPromptPicker(host) {
    window.bfPromptStyles();
    let chosen = PROMPTS[0].key;
    const box = document.createElement('div');
    box.className = 'gg-prompts';
    box.setAttribute('role', 'group');
    box.setAttribute('aria-label', 'What are you answering?');
    const buttons = PROMPTS.map(p => {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = p.label;
      b.setAttribute('aria-pressed', String(p.key === chosen));
      b.addEventListener('click', () => {
        chosen = p.key;
        buttons.forEach(other => other.setAttribute('aria-pressed', String(other === b)));
      });
      box.appendChild(b);
      return b;
    });
    host.appendChild(box);
    return () => chosen;
  };
})();
