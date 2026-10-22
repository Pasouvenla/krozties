// Ce que vaut chaque caractéristique : la même rotation recalculée avec un cran
// de plus, puis le gain ramené à la Puissance. Trois conseils en tête, le détail
// replié dessous.

import { echapper, nb } from './format.js';

/// Les crans mesurés : le pas ajouté, et le nom d'un point de la
/// caractéristique.
export const CRANS = {
  pa: { pas: 1, nom: 'PA' },
  puissance: { pas: 100, nom: 'Puissance' },
  force: { pas: 100, nom: 'Force' },
  intelligence: { pas: 100, nom: 'Intelligence' },
  chance: { pas: 100, nom: 'Chance' },
  agilite: { pas: 100, nom: 'Agilité' },
  dommages: { pas: 10, nom: 'Dommages' },
  dommages_terre: { pas: 10, nom: 'Dommages Terre' },
  dommages_feu: { pas: 10, nom: 'Dommages Feu' },
  dommages_eau: { pas: 10, nom: 'Dommages Eau' },
  dommages_air: { pas: 10, nom: 'Dommages Air' },
  dommages_neutre: { pas: 10, nom: 'Dommages Neutre' },
  critique: { pas: 5, nom: '% Critique' },
  dommages_critiques: { pas: 10, nom: 'Dommages critiques' },
  sorts: { pas: 5, nom: '% Dommages aux sorts' },
  distance: { pas: 5, nom: '% Dommages distance' },
  melee: { pas: 5, nom: '% Dommages mêlée' },
  armes: { pas: 5, nom: '% Dommages d\'armes' },
  poussee: { pas: 20, nom: 'Dommages Poussée' },
};

/// La caractéristique et les dommages de chaque élément : la Force porte la
/// Terre et le Neutre.
const PAR_ELEMENT = {
  Earth: ['force', 'dommages_terre'],
  Neutral: ['force', 'dommages_neutre'],
  Fire: ['intelligence', 'dommages_feu'],
  Water: ['chance', 'dommages_eau'],
  Air: ['agilite', 'dommages_air'],
};

/// Ce qui vaut la peine d'être mesuré pour cette rotation : les éléments
/// qu'elle frappe vraiment, l'arme si elle frappe, la poussée si elle est
/// comptée. `elementsDe` dit les éléments d'une source de la répartition,
/// `['meilleur']` pour un sort qui frappe dans le meilleur élément du build, et
/// rien pour ce qui ne suit pas ses caractéristiques (une invocation).
/// `orientation` : les éléments du build, où frappe un sort au meilleur
/// élément.
export function aMesurer(r, demande, elementsDe, orientation) {
  const elements = new Set();
  let arme = false;
  for (const p of r.repartition || []) {
    if (!(p.damage > 0)) continue;
    if (p.id === 'arme') arme = true;
    const e = elementsDe(p.id);
    (e.includes('meilleur') ? orientation : e).filter((x) => x in PAR_ELEMENT).forEach((x) => elements.add(x));
  }
  const par = [...elements].flatMap((e) => PAR_ELEMENT[e]);
  const liste = ['pa', 'puissance', ...new Set(par.filter((s) => !s.startsWith('dommages_'))), 'dommages',
    ...new Set(par.filter((s) => s.startsWith('dommages_'))), 'critique', 'dommages_critiques', 'sorts', 'distance', 'melee'];
  if (arme) liste.push('armes');
  if (demande.poussees_bloquees) liste.push('poussee');
  return liste;
}

/// Les éléments du build : ceux dont la caractéristique atteint la moitié de la
/// plus forte, comme l'orientation du deck. `elements` : ceux de la fiche
/// résolue, par leur nom en jeu.
export function orientationDu(elements) {
  const NOMS = { Terre: 'Earth', Feu: 'Fire', Eau: 'Water', Air: 'Air', Neutre: 'Neutral' };
  const plusForte = Math.max(0, ...(elements || []).map((e) => e.characteristic));
  return (elements || []).filter((e) => plusForte > 0 && e.characteristic * 2 >= plusForte)
    .map((e) => NOMS[e.name]).filter(Boolean);
}

const pourcent = (x) => `${x >= 0 ? '+' : '−'}${Math.abs(x * 100).toFixed(1).replace('.', ',')} %`;
const taux = (x) => (Math.abs(x) >= 10 ? nb(x) : nb(x, 1));

/// Les mesures, chacune ramenée à son pas et à la Puissance.
export function lire(base, totaux) {
  const lignes = Object.entries(totaux).map(([stat, total]) => {
    const { pas, nom } = CRANS[stat];
    const gain = total - base;
    return { stat, nom, pas, gain, part: base > 0 ? gain / base : 0, parPoint: gain / pas };
  });
  const puissance = lignes.find((l) => l.stat === 'puissance');
  const unite = puissance && puissance.parPoint > 0 ? puissance.parPoint : null;
  for (const l of lignes) l.enPuissance = unite ? l.parPoint / unite : null;
  return lignes;
}

/// Le cran, dit comme on l'ajoute : « +1 PA », « +100 Intelligence », « +5 %
/// Critique ».
const cranDit = (l) => `+${l.pas} ${l.nom}`;

/// Les trois conseils, puis le taux de change replié. `critique` : le
/// critique du build, qui dit si un critique sans effet est au plafond.
export function dessiner(lignes, critique) {
  const utiles = lignes.filter((l) => l.gain >= 0.5);
  const nuls = lignes.filter((l) => l.gain < 0.5);
  if (!utiles.length) return '<p class="aide">Aucune caractéristique mesurée ne change le total.</p>';
  const meilleur = [...utiles].sort((a, b) => b.part - a.part)[0];
  const caracs = utiles.filter((l) => ['force', 'intelligence', 'chance', 'agilite'].includes(l.stat))
    .sort((a, b) => b.part - a.part);
  const critiqueNul = nuls.find((l) => l.stat === 'critique');
  const tuile = (titre, grand, detail, classe = '') => `
    <div class="tuile-valeur${classe}"><small>${titre}</small><b>${echapper(grand)}</b><span>${echapper(detail)}</span></div>`;
  const equivalent = (l) => (l.stat !== 'puissance' && l.enPuissance != null ? `, autant que ${taux(l.enPuissance * l.pas)} Puissance` : '');
  const tuiles = [
    tuile('Le plus rentable', cranDit(meilleur), `${pourcent(meilleur.part)}${equivalent(meilleur)}`),
    caracs.length
      ? tuile('Votre caractéristique', caracs[0].nom, `${pourcent(caracs[0].part)} pour 100 points${
        caracs[1] ? `, contre ${pourcent(caracs[1].part)} en ${caracs[1].nom}` : ''}`)
      : tuile('Votre caractéristique', 'Aucune', 'vos dégâts ne suivent aucune caractéristique', ' non'),
    critiqueNul && critique > 0
      ? tuile('Inutile ici', 'Critique', 'vos sorts sont déjà au plafond', ' non')
      : nuls.length
        ? tuile('Inutile ici', nuls[0].nom, 'ne change pas le total', ' non')
        : tuile('Inutile ici', 'Rien', 'chaque caractéristique mesurée rapporte', ' non'),
  ];
  const parTaux = utiles.filter((l) => l.stat !== 'puissance' && l.enPuissance != null)
    .sort((a, b) => b.enPuissance - a.enPuissance);
  const rien = nuls.map((l) => (l.stat === 'critique' && critique > 0 ? `${l.nom} (au plafond)` : l.nom));
  return `<div class="tuiles-valeurs">${tuiles.join('')}</div>
    ${parTaux.length ? `<details class="hypotheses"><summary>Toutes les caractéristiques, en Puissance</summary>
      <table class="mini valeurs"><tbody>${parTaux.map((l) => `
        <tr><td>1 ${echapper(l.nom)}</td><td class="taux">${taux(l.enPuissance)}</td>
          <td class="attenue">${pourcent(l.part)} pour ${cranDit(l)}</td></tr>`).join('')}
      </tbody></table>
      ${rien.length ? `<p class="aide">Rien de plus avec : ${rien.map(echapper).join(', ')}.</p>` : ''}
    </details>` : ''}`;
}
