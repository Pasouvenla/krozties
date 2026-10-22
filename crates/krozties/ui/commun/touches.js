// Les touches du compagnon de combat : trois gestes, une touche chacun, gardées
// dans le stockage local que la Rotation et le compagnon partagent. Une touche
// s'écrit comme le module des raccourcis globaux la lit : ses modificateurs,
// puis le code de la touche (« Shift+F8 »).

import { appeler, hote } from '../pont.js';
import { echapper } from './format.js';

export const CLE_TOUCHES = 'krozties.compagnon.touches';

export const GESTES = [
  { id: 'suivant', nom: 'Sort suivant', court: 'suivant' },
  { id: 'tour', nom: 'Tour suivant', court: 'tour' },
  { id: 'precedent', nom: 'Revenir en arrière', court: 'retour' },
];

export const D_ORIGINE = { suivant: 'F8', tour: 'Shift+F8', precedent: 'F7' };

const SUR_MAC = typeof navigator !== 'undefined'
  && /Mac/.test(navigator.platform || navigator.userAgent || '');

/// Chaque modificateur : son nom pour le module, son champ dans un appui, son
/// nom sur le clavier. Dans l'ordre où le système les écrit.
const MODIFICATEURS = [
  ['Control', 'ctrlKey', 'Ctrl'],
  ['Alt', 'altKey', SUR_MAC ? 'Option' : 'Alt'],
  ['Shift', 'shiftKey', 'Maj'],
  ['Super', 'metaKey', SUR_MAC ? 'Cmd' : 'Win'],
];

const NOMS = {
  Space: 'Espace', Enter: 'Entrée', Tab: 'Tab', Backspace: 'Retour arrière', Escape: 'Échap',
  ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', PageUp: 'Page préc.',
  PageDown: 'Page suiv.', Home: 'Début', End: 'Fin', Insert: 'Inser', Delete: 'Suppr',
};

export function lireLesTouches() {
  try {
    return { ...D_ORIGINE, ...JSON.parse(localStorage.getItem(CLE_TOUCHES) || '{}') };
  } catch (e) {
    return { ...D_ORIGINE };
  }
}

function ecrireLesTouches(touches) {
  try {
    localStorage.setItem(CLE_TOUCHES, JSON.stringify(touches));
  } catch (e) { /* le réglage vaut jusqu'à la fermeture */ }
}

/// Une touche telle qu'on la lit sur le clavier : « Maj+F8 », « Ctrl+A ».
export function libelle(touche) {
  const parties = touche.split('+');
  const code = parties.pop();
  const modificateurs = parties.map((m) => (MODIFICATEURS.find(([id]) => id === m) || [m, '', m])[2]);
  let nom = NOMS[code] || code;
  if (/^Key[A-Z]$/.test(code)) nom = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) nom = code.slice(5);
  else if (/^Numpad[0-9]$/.test(code)) nom = `Pavé ${code.slice(6)}`;
  return [...modificateurs, nom].join('+');
}

/// La touche d'un appui, avec la raison de la refuser s'il y en a une ; rien
/// tant que l'on ne presse qu'un modificateur.
export function depuisLAppui(e) {
  if (!e.code || ['Shift', 'Control', 'Alt', 'Meta', 'OS'].includes(e.key)) return null;
  const modificateurs = MODIFICATEURS.filter(([, champ]) => e[champ]).map(([id]) => id);
  const touche = [...modificateurs, e.code].join('+');
  if (!modificateurs.length && !/^F([1-9]|1[0-9]|2[0-4])$/.test(e.code)) {
    return { touche, refus: `« ${libelle(touche)} » seule n'arriverait plus au jeu : ajoutez Maj, Ctrl ou ${
      SUR_MAC ? 'Option' : 'Alt'}, ou prenez une touche F.` };
  }
  return { touche };
}

/// Le geste que déclenche un appui, s'il en déclenche un.
export function gesteDe(e, touches) {
  const appui = depuisLAppui(e);
  return appui ? (GESTES.find((g) => touches[g.id] === appui.touche) || {}).id || null : null;
}

const resume = (t) => `Raccourcis du compagnon : ${GESTES.map((g) => libelle(t[g.id])).join(', ')}`;

/// Le repli des réglages, sous le bouton du compagnon.
export function reglages() {
  const t = lireLesTouches();
  return `<details class="hypotheses touches-compagnon" id="touchesCompagnon">
    <summary>${echapper(resume(t))}</summary>
    <div class="touches">${GESTES.map((g) => `
      <div class="touche"><span>${g.nom}</span><button type="button" class="capuchon" data-geste="${g.id}">${
        echapper(libelle(t[g.id]))}</button></div>`).join('')}
    </div>
    <p class="refus-touche" role="status"></p>
    <div class="pied-touches">${SUR_MAC
      ? '<small>Sur Mac, les touches F se pressent avec Fn, sauf si elles sont réglées en touches de fonction standard.</small>'
      : '<span></span>'}
      <button type="button" class="lien" data-action="origine">Rétablir ${
        echapper(GESTES.map((g) => libelle(D_ORIGINE[g.id])).join(', '))}</button>
    </div>
  </details>`;
}

/// Les touches que le compagnon ouvert prend ; sans touches, il les rend le
/// temps d'en presser une nouvelle. Rend les gestes dont une autre application
/// tient déjà la touche.
async function appliquer(touches) {
  if (hote !== 'bureau') return [];
  const r = await appeler('compagnonTouches', { touches });
  return (r && r.prises) || [];
}

/// Les réglages en marche : un clic sur une touche, puis la nouvelle touche
/// pressée ; Échap ou un clic ailleurs annule.
export function brancherLesReglages(racine) {
  const message = racine.querySelector('.refus-touche');
  const dire = (texte) => { message.textContent = texte || ''; };
  let enAttente = null;

  const redessiner = () => {
    const t = lireLesTouches();
    racine.querySelector('summary').textContent = resume(t);
    racine.querySelectorAll('.capuchon').forEach((b) => {
      b.textContent = libelle(t[b.dataset.geste]);
      b.classList.remove('ecoute');
    });
  };

  const garder = async (touches) => {
    const prises = await appliquer(touches);
    if (prises.length) {
      dire(`Une autre application tient déjà ${libelle(touches[prises[0]])} : choisissez-en une autre.`);
      return false;
    }
    ecrireLesTouches(touches);
    dire('');
    return true;
  };

  async function arreter(annule) {
    if (!enAttente) return;
    enAttente = null;
    document.removeEventListener('keydown', surAppui, true);
    document.removeEventListener('mousedown', ailleurs, true);
    redessiner();
    if (annule) await appliquer(lireLesTouches()).catch(() => {});
  }

  function ailleurs(e) {
    if (!e.target.closest || !e.target.closest('.capuchon')) arreter(true);
  }

  async function surAppui(e) {
    e.preventDefault();
    e.stopPropagation();
    if (!racine.isConnected || e.key === 'Escape') {
      dire('');
      await arreter(true);
      return;
    }
    const appui = depuisLAppui(e);
    if (!appui || !enAttente) return;
    if (appui.refus) {
      dire(appui.refus);
      return;
    }
    const t = lireLesTouches();
    const geste = enAttente.dataset.geste;
    const autre = GESTES.find((g) => g.id !== geste && t[g.id] === appui.touche);
    if (autre) {
      dire(`${libelle(appui.touche)} sert déjà à « ${autre.nom} ».`);
      return;
    }
    try {
      if (await garder({ ...t, [geste]: appui.touche })) await arreter(false);
    } catch (err) {
      dire(err.message);
      await arreter(true);
    }
  }

  racine.addEventListener('click', async (e) => {
    const capuchon = e.target.closest('.capuchon');
    if (capuchon) {
      await arreter(true);
      enAttente = capuchon;
      capuchon.classList.add('ecoute');
      capuchon.textContent = 'Pressez une touche…';
      dire('');
      document.addEventListener('keydown', surAppui, true);
      document.addEventListener('mousedown', ailleurs, true);
      // Les touches du compagnon ouvert se taisent, sinon la nouvelle n'arriverait
      // jamais ici.
      await appliquer(null).catch(() => {});
      return;
    }
    if (e.target.closest('[data-action=origine]')) {
      await arreter(false);
      try {
        await garder({ ...D_ORIGINE });
      } catch (err) {
        dire(err.message);
      }
      redessiner();
    }
  });
}

/// Le compagnon a trouvé des touches qu'une autre application tient : le
/// repli s'ouvre et le dit.
export function signalerLesPrises(racine, prises) {
  if (!racine || !prises || !prises.length) return;
  const t = lireLesTouches();
  racine.open = true;
  racine.querySelector('.refus-touche').textContent = `Une autre application tient déjà ${
    prises.map((id) => libelle(t[id])).join(', ')} : choisissez-en une autre.`;
}
