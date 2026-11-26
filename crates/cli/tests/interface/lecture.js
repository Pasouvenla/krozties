// The companion's reading controls, checked in a browser under a stand-in for
// the desktop app: the first activation through the fiche, a refused
// permission and a retry, the pastille, the ◉ toggle; then a reading turned on
// again with the companion, in the bar and in the list.

const results = { errors: [] };
window.addEventListener('error', (event) => results.errors.push(String(event.message)));
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const KEY = 'krozties.compagnon.lecture';
const DENIED = 'Autorisez Krozties, puis relancez-le.';

async function open(storage) {
  localStorage.clear();
  for (const [key, value] of Object.entries(storage)) localStorage.setItem(key, value);
  const frame = document.createElement('iframe');
  frame.src = 'compagnon-essai.html';
  document.body.append(frame);
  for (let i = 0; i < 200 && !(frame.contentDocument && frame.contentDocument.querySelector('.cp')); i += 1) {
    await pause(50);
  }
  const doc = frame.contentDocument;
  const win = frame.contentWindow;
  return {
    frame,
    doc,
    win,
    calls: () => win.appels.filter((a) => a.commande === 'lire_le_combat').map((a) => a.args.actif),
    toggle: () => doc.querySelector('[data-action=lecture]'),
    stored: () => localStorage.getItem(KEY),
    pill: () => {
      const p = doc.querySelector('.cp-pastille');
      return p ? { text: p.textContent, title: p.title, button: p.tagName === 'BUTTON', kind: p.className } : null;
    },
    fiche: () => {
      const f = doc.querySelector('.cp-fiche');
      return f ? {
        title: f.querySelector('.cp-tete b')?.textContent ?? null,
        error: f.querySelector('.cp-erreur')?.textContent ?? null,
        buttons: [...f.querySelectorAll('.cp-boutons button')].map((b) => b.textContent + (b.disabled ? ' (off)' : '')),
      } : null;
    },
  };
}

// Never turned on, no rotation yet.
const c = await open({});
results.fresh = { toggle: Boolean(c.toggle()), pressed: c.toggle()?.getAttribute('aria-pressed') ?? null, pill: c.pill() };
c.toggle().click();
results.offered = { fiche: c.fiche(), calls: c.calls() };
c.doc.querySelector('[data-action=lecture-fermer]').click();
results.later = { fiche: c.fiche(), calls: c.calls(), stored: c.stored(), pill: c.pill() };
c.toggle().click();
c.doc.querySelector('[data-action=lecture-activer]').click();
await pause(20);
results.activating = { fiche: c.fiche(), calls: c.calls(), stored: c.stored() };
c.win.compagnonLecture({ state: 'denied', game: false, error: DENIED });
results.denied = { fiche: c.fiche() };
c.doc.querySelector('[data-action=lecture-activer]').click();
await pause(20);
results.retrying = { fiche: c.fiche(), calls: c.calls() };
c.win.compagnonLecture({ state: 'off', game: false, error: null });
results.stillWaiting = Boolean(c.fiche());
c.win.compagnonLecture({ state: 'waiting', game: false, error: null });
results.waiting = { fiche: c.fiche(), pill: c.pill(), pressed: c.toggle().getAttribute('aria-pressed') };
c.win.compagnonLecture({ state: 'following', game: true, error: null });
results.following = c.pill();
c.toggle().click();
await pause(20);
results.off = { pill: c.pill(), calls: c.calls(), stored: c.stored(), pressed: c.toggle().getAttribute('aria-pressed') };
c.toggle().click();
await pause(20);
results.onAgain = { fiche: c.fiche(), calls: c.calls(), stored: c.stored() };
results.errors.push(...c.win.erreurs);
c.frame.remove();

// Turned on last time, with a rotation: it resumes with the companion.
const rotation = { calcule_le: 1, tours: [{ pa: 6, degats: 120, lancers: [{ nom: 'Flèche', pa: 3, indications: [] }] }] };
const r = await open({ [KEY]: 'true', 'krozties.compagnon': JSON.stringify(rotation) });
await pause(20);
results.resumed = { calls: r.calls(), pill: r.pill() };
r.win.compagnonLecture({ state: 'following', game: true, error: null });
results.bar = [...r.doc.querySelector('.cp-barre').children].map((e) => e.className.split(' ')[0]);
r.doc.querySelector('[data-action=disposition]').click();
results.list = Boolean(r.doc.querySelector('.cp-liste .cp-lecture .cp-pastille'));
r.win.compagnonLecture({ state: 'denied', game: false, error: DENIED });
results.stopped = r.pill();
r.doc.querySelector('[data-action=lecture-arret]').click();
results.explained = r.fiche();
r.doc.querySelector('[data-action=lecture-fermer]').click();
results.closed = { fiche: r.fiche(), list: Boolean(r.doc.querySelector('.cp-liste')) };
results.errors.push(...r.win.erreurs);
r.frame.remove();
localStorage.clear();

const out = document.createElement('pre');
out.id = 'lecture-essai';
out.textContent = JSON.stringify(results);
document.body.append(out);
