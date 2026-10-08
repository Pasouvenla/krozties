// Les deux graphiques de la rotation : les colonnes disent si la rotation tient
// un régime, la courbe ce qu'un ennemi de plus rapporte vraiment.

import { echapper, nb } from './format.js';

const L = 700;
const H = 190;

/// Des graduations à des nombres ronds, environ quatre intervalles. Le plancher
/// évite `Math.log10(0)`, qui vaudrait `-Infinity` et viderait le graphique.
export function echelle(maxi) {
  // Un maximum nul ou absent retombe sur une échelle de 0 à 4, qui se LIT,
  // plutôt que sur cinq graduations toutes marquées zéro.
  const cible = Number(maxi) > 0 ? Number(maxi) : 4;
  const brut = cible / 4;
  const ordre = 10 ** Math.floor(Math.log10(brut));
  const pas = [1, 2, 2.5, 5, 10].map((m) => m * ordre).find((c) => c >= brut) || ordre * 10;
  return { pas, haut: Math.ceil(cible / pas) * pas };
}

const aireDe = (m) => ({ w: L - m.g - m.d, h: H - m.h - m.b });

// Grille horizontale et axe des ordonnées : les deux graphiques partagent la
// même, et c'est ce qui les rend comparables d'un coup d'oeil.
function fond(m, aire, ech) {
  let sortie = '';
  for (let v = 0; v <= ech.haut + 0.001; v += ech.pas) {
    const y = (m.h + aire.h - (v / ech.haut) * aire.h).toFixed(1);
    sortie += `<line class="g-grille" x1="${m.g}" x2="${L - m.d + 4}" y1="${y}" y2="${y}"/>`
      + `<text class="g-axe" x="${m.g - 6}" y="${(Number(y) + 3).toFixed(1)}" text-anchor="end">${nb(v)}</text>`;
  }
  return sortie;
}

const enveloppe = (legende, corps, note) => `<figure class="graphique">`
  + (legende ? `<figcaption>${legende}</figcaption>` : '')
  + `<svg viewBox="0 0 ${L} ${H}">${corps}</svg>`
  + (note || '')
  + `</figure>`;

// ---------------------------------------------------------------------
// Dégâts par tour
// ---------------------------------------------------------------------
// Des colonnes : la couleur sépare les tours d'ouverture, joués une fois, des
// tours de boucle ; le pointillé est la moyenne de la boucle. Tout ce qui est
// chiffré dépend du nombre d'ennemis, et le rappelle.
/// La coupe ne tombe jamais dans la boucle : les cycles sont réservés d'abord,
/// l'ouverture prend la place qui reste, et ce qui est retiré se dit.
const MAX_COLONNES = 24;

export function parTour(st, tours, cibles) {
  const stable = st || {};
  const cycle = stable.cycle || [];
  let suite = [];
  let coupes = 0;
  let premierCoupe = 0;
  if (stable.cycle_length && cycle.length) {
    // Deux passages au moins : un seul ne se distingue pas d'une ouverture qui
    // continuerait. Trois quand le cycle est court, pour que le motif s'impose.
    let cycles = Math.max(2, Math.min(3, Math.ceil(8 / stable.cycle_length)));
    while (cycles > 1 && cycles * cycle.length > MAX_COLONNES) cycles -= 1;
    const ouverture = stable.opener || [];
    const place = Math.max(0, MAX_COLONNES - cycles * cycle.length);
    coupes = Math.max(0, ouverture.length - place);
    premierCoupe = place + 1;
    suite = ouverture.slice(0, place).map((t, i) => ({ v: t.damage, phase: 'ouverture', tour: i + 1 }));
    for (let c = 0; c < cycles; c += 1) {
      cycle.forEach((t, j) => suite.push({
        v: t.damage,
        phase: 'boucle',
        // Le NUMÉRO RESTE VRAI même quand des tours manquent : un axe qui se
        // renumérote après une coupe ferait lire la boucle au mauvais tour.
        tour: ouverture.length + c * cycle.length + j + 1,
      }));
    }
  } else {
    suite = (tours || []).map((t, i) => ({
      v: t.damage, phase: 'boucle', tour: t.turn != null ? t.turn : i + 1,
    }));
  }
  // Dernier rempart : un cycle plus long que le cadre passait quand même,
  // puisque le compte de cycles ne descend pas en dessous d'un. Alors rien ne
  // peut montrer la répétition, et c'est cela qu'il faut dire.
  const deborde = suite.length > MAX_COLONNES;
  if (deborde) suite = suite.slice(0, MAX_COLONNES);
  if (!suite.length) return '';

  // La marge droite loge l'étiquette de moyenne : la poser sur le tracé la
  // ferait chevaucher la dernière colonne.
  const m = { h: 14, b: 26, g: 46, d: 78 };
  const aire = aireDe(m);
  const ech = echelle(Math.max(...suite.map((d) => d.v), stable.per_turn || 0));
  const y = (v) => m.h + aire.h - (v / ech.haut) * aire.h;

  const bande = aire.w / suite.length;
  const epaisseur = Math.min(24, bande - 2); // 2 px de fond entre deux colonnes

  const barres = suite.map((d, i) => {
    const x = m.g + i * bande + (bande - epaisseur) / 2;
    const t = y(d.v);
    const hauteur = Math.max(2, m.h + aire.h - t);
    const r = Math.min(4, epaisseur / 2);
    // Sommet arrondi, base carrée : le pied de la colonne EST la ligne zéro,
    // l'arrondir décollerait la donnée de son axe.
    const chemin = `M${x} ${t + hauteur}V${t + r}a${r} ${r} 0 0 1 ${r} ${-r}`
      + `h${epaisseur - 2 * r}a${r} ${r} 0 0 1 ${r} ${r}V${t + hauteur}Z`;
    return `<path class="g-barre ${d.phase === 'boucle' ? 'g-boucle' : 'g-ouverture'}"`
      + ` d="${chemin}" tabindex="0"><title>Tour ${d.tour} · ${nb(d.v)} dégâts · ${d.phase}</title></path>`;
  }).join('');

  // Étiqueter le tour le plus fort et le plus faible, pas tous : un nombre sur
  // chaque colonne ne se lit pas, mais une valeur qu'on n'atteint qu'au survol
  // n'existe pas pour qui n'a pas de souris. Ces deux-là donnent l'amplitude.
  const fort = suite.reduce((a, b) => (b.v > a.v ? b : a));
  const faible = suite.reduce((a, b) => (b.v < a.v ? b : a));
  const reperes = [fort, faible]
    .filter((d, i, tab) => tab.indexOf(d) === i)
    .map((d) => `<text class="g-repere" x="${(m.g + suite.indexOf(d) * bande + bande / 2).toFixed(1)}"`
      + ` y="${(y(d.v) - 5).toFixed(1)}" text-anchor="middle">${nb(d.v)}</text>`).join('');

  const axeX = suite.map((d, i) => `<text class="g-axe" x="${(m.g + i * bande + bande / 2).toFixed(1)}"`
    + ` y="${H - 8}" text-anchor="middle">${d.tour}</text>`).join('');

  const moyenne = stable.per_turn
    ? `<line class="g-moyenne" x1="${m.g}" x2="${L - m.d + 4}" y1="${y(stable.per_turn).toFixed(1)}"`
      + ` y2="${y(stable.per_turn).toFixed(1)}"/>`
      + `<text class="g-moyenne-texte" x="${L - m.d + 10}" y="${(y(stable.per_turn) + 3.5).toFixed(1)}">`
      + `${nb(stable.per_turn)} / tour</text>`
    : '';

  const legende = '<div class="g-legende">'
    + (suite.some((d) => d.phase === 'ouverture')
      ? '<span><i class="g-puce g-p-ouverture"></i>Ouverture, jouée une fois</span>' : '')
    + '<span><i class="g-puce g-p-boucle"></i>Boucle, répétée</span>'
    + (stable.per_turn ? '<span><i class="g-puce g-p-moyenne"></i>Moyenne de la boucle</span>' : '')
    + '</div>';

  // Une phrase seulement quand elle s'applique : le nombre d'ennemis, dont tout
  // dépend, et les tours que le cadre ne montre pas.
  return enveloppe(
    [
      cibles > 1 ? `Sur ${cibles} ennemis.` : '',
      coupes > 1 ? `Tours ${premierCoupe} à ${premierCoupe + coupes - 1} non affichés.`
        : coupes ? `Tour ${premierCoupe} non affiché.` : '',
      deborde ? `Tours après le ${suite[suite.length - 1].tour} non affichés.` : '',
    ].filter(Boolean).join(' '),
    fond(m, aire, ech) + barres + reperes + axeX + moyenne,
    legende,
  );
}

// ---------------------------------------------------------------------
// Dégâts selon le nombre d'ennemis
// ---------------------------------------------------------------------
// Une courbe : chaque point est une résolution complète. La droite pointillée
// est la borne haute, si chaque point de dégât montait avec le nombre
// d'ennemis ; l'écart dit quelle part des dégâts n'en profite pas.
export function selonCibles(balayage, choisi) {
  const pts = (balayage || []).filter((p) => !p.error && p.per_turn != null);
  if (pts.length < 2) return '';

  const m = { h: 14, b: 30, g: 46, d: 78 };
  const aire = aireDe(m);
  const base = pts[0].per_turn;
  const lineaire = pts.map((p) => base * p.targets);
  const ech = echelle(Math.max(...pts.map((p) => p.per_turn), ...lineaire));
  const y = (v) => m.h + aire.h - (v / ech.haut) * aire.h;
  const x = (i) => m.g + (pts.length === 1 ? aire.w / 2 : (i / (pts.length - 1)) * aire.w);

  const trace = (vals, cls) => `<path class="${cls}" d="`
    + vals.map((v, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(' ') + '"/>';

  const points = pts.map((p, i) => {
    const actif = p.targets === choisi;
    return `<circle class="${actif ? 'g-point-actif' : 'g-point'}" cx="${x(i).toFixed(1)}"`
      + ` cy="${y(p.per_turn).toFixed(1)}" r="${actif ? 5 : 3.5}"><title>${p.targets}`
      + ` ${p.targets > 1 ? 'ennemis' : 'ennemi'} · ${nb(p.per_turn)} dégâts par tour</title></circle>`;
  }).join('');

  const axeX = pts.map((p, i) => `<text class="g-axe" x="${x(i).toFixed(1)}" y="${H - m.b + 16}"`
    + ` text-anchor="middle">${p.targets}</text>`).join('');

  // L'étiquette de droite chiffre le gain, qui est la lecture utile : « à trois
  // ennemis vous faites 2,4 fois ce que vous faites sur un ».
  const dernier = pts[pts.length - 1];
  const facteur = base > 0 ? dernier.per_turn / base : 0;
  const etiquette = `<text class="g-moyenne-texte" x="${L - m.d + 8}"`
    + ` y="${(y(dernier.per_turn) + 3).toFixed(1)}">× ${facteur.toFixed(2).replace('.', ',')}</text>`;

  // Le changement de rotation se dit quand il a lieu. Les ennemis sont supposés
  // serrés autour du point d'impact : la courbe ne simule pas la dégressivité, et
  // la page le dit.
  const memeCycle = pts.every((p) => (p.cycle || []).join('|') === (pts[0].cycle || []).join('|'));
  const note = `<p class="aide">${memeCycle
    ? `Même rotation de ${pts[0].targets} à ${dernier.targets} ennemis.`
    : "Rotation différente selon le nombre d'ennemis."}`
    + ' Ennemis groupés au centre de la zone.</p>';

  // Un point qui a échoué ne disparaît pas en silence : une courbe à trous se
  // lirait comme un plateau.
  const rates = (balayage || []).filter((p) => p.error);
  const echecs = rates.length
    ? `<p class="aide alerte">Aucun résultat à ${rates.map((p) => p.targets).join(', ')} `
      + `${rates.length > 1 ? 'ennemis' : 'ennemi'} : ${echapper(rates[0].error)}</p>`
    : '';

  // La légende ne redit pas le titre de la carte : elle explique le pointillé,
  // qui est la seule chose du dessin qu'on ne devine pas.
  return enveloppe(
    'Dégâts par tour · en pointillé, si chaque sort touchait tous les ennemis',
    fond(m, aire, ech) + axeX + trace(lineaire, 'g-lineaire')
    + trace(pts.map((p) => p.per_turn), 'g-courbe') + points + etiquette,
    note + echecs,
  );
}
