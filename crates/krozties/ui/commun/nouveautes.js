// Les notes de version que l'application embarque : la page « Notes de mise à
// jour », et la fenêtre qui présente une seule fois les nouveautés d'une
// version, à son premier lancement.

import { echapper, pluriel } from './format.js';
import { plusRecente } from './mise-a-jour.js';

const CLE = 'krozties.notes';
const MOIS = new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' });

let lecture = null;

/// Les versions publiées, de la plus récente à la plus ancienne, sans la
/// documentation, qui ne concerne pas le jeu.
export function charger() {
  lecture ||= fetch('notes-de-version.json')
    .then((r) => (r.ok ? r.json() : { versions: [] }))
    .then((d) => (d.versions || [])
      .filter((v) => v.version !== 'prochaine')
      .map((v) => ({ ...v, rubriques: (v.rubriques || []).filter((r) => r.titre !== 'Documentation' && r.lignes.length) })))
    .catch(() => []);
  return lecture;
}

export const dateLongue = (iso) => (iso ? MOIS.format(new Date(`${iso}T00:00:00Z`)) : '');

export const nombre = (v) => v.rubriques.reduce((n, r) => n + r.lignes.length, 0);

export const changements = (v) => `${nombre(v)} ${pluriel(nombre(v), 'changement')}`;

export function rubriques(v) {
  return v.rubriques.map((r) => `<p class="rubrique">${echapper(r.titre)}</p>
    <ul>${r.lignes.map((l) => `<li>${echapper(l)}</li>`).join('')}</ul>`).join('');
}

/// Les versions après `vue`, jusqu'à `actuelle` comprise.
export const entre = (versions, vue, actuelle) => versions
  .filter((v) => plusRecente(v.version, vue) && !plusRecente(v.version, actuelle));

function lue() {
  try {
    return (JSON.parse(localStorage.getItem(CLE)) || {}).lue || null;
  } catch (e) {
    return null;
  }
}

function marquer(version) {
  try {
    localStorage.setItem(CLE, JSON.stringify({ lue: version }));
  } catch (e) {
    // Stockage refusé : la fenêtre reviendra au prochain lancement.
  }
}

/// La fenêtre des nouveautés de `versions`, la plus récente d'abord.
export function fenetre(versions, { fermer, toutes }) {
  document.querySelector('.maj')?.remove();
  const [derniere] = versions;
  const el = document.createElement('aside');
  el.className = 'maj nouveautes';
  el.setAttribute('role', 'dialog');
  el.setAttribute('aria-label', `Mise à jour ${derniere.version}`);
  el.innerHTML = `
    <strong>Mise à jour ${echapper(derniere.version)}</strong>
    <span class="date">${dateLongue(derniere.date)}</span>
    <div class="notes">${versions.map((v) => `${versions.length > 1
      ? `<p class="version-sautee">${echapper(v.version)}</p>` : ''}${rubriques(v)}`).join('')}</div>
    <div class="actions">
      <button type="button" class="compris">Compris</button>
      <button type="button" class="toutes">Toutes les notes</button>
    </div>`;
  el.querySelector('.compris').addEventListener('click', () => { el.remove(); fermer(); });
  el.querySelector('.toutes').addEventListener('click', () => { el.remove(); fermer(); toutes(); });
  document.body.appendChild(el);
  return el;
}

/// Au lancement de l'application de bureau : les nouveautés des versions
/// arrivées depuis la dernière lecture. Une première installation n'a rien à
/// rattraper ; une version déjà utilisée avant cette fenêtre (sa vérification
/// de mise à jour a laissé sa trace) montre celle qui est installée.
export async function montrer(appeler, afficher) {
  const actuelle = await appeler('version');
  const vue = lue();
  if (!vue && localStorage.getItem('krozties.mise-a-jour') === null) {
    marquer(actuelle);
    return;
  }
  const versions = await charger();
  const aLire = vue ? entre(versions, vue, actuelle) : versions.filter((v) => v.version === actuelle);
  if (!aLire.some(nombre)) {
    marquer(actuelle);
    return;
  }
  await new Promise((fin) => {
    fenetre(aLire.filter(nombre), {
      fermer: () => { marquer(actuelle); fin(); },
      toutes: () => afficher('notes'),
    });
  });
}
