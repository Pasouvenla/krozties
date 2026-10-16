// Les graphiques de la Rotation et de la Comparaison : le rythme des tours, la
// rentabilité de chaque PA, ce qu'un ennemi de plus rapporte, et ce qui sépare
// deux builds. Leurs couleurs viennent de la palette (`--serie-1`, `--serie-2`).

import { echapper, nb, pluriel } from './format.js';

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
// Le rythme de la rotation
// ---------------------------------------------------------------------
// Trois repères, le tour le plus fort, le plus faible et la moyenne, puis les
// dégâts de chaque tour empilés : ce que le tour lance, et ce qui frappe à son
// début (explosion différée, poison, invocations). Le pointillé est la moyenne
// sur la durée, quand la rotation tient une boucle.
const point = (x, y) => `${x.toFixed(1)} ${y.toFixed(1)}`;

export function rythme(tours, st, cibles) {
  const suite = (tours || []).map((t, i) => ({
    tour: t.turn != null ? t.turn : i + 1,
    total: t.damage,
    debut: Math.min(t.damage, (t.opening_sources || []).reduce((n, o) => n + (o.damage || 0), 0)),
  }));
  if (!suite.length) return '';
  const duree = st && st.per_turn ? st.per_turn : null;
  const moyenne = suite.reduce((s, d) => s + d.total, 0) / suite.length;
  const fort = suite.reduce((a, b) => (b.total > a.total ? b : a));
  const faible = suite.reduce((a, b) => (b.total < a.total ? b : a));
  const tuile = (titre, valeur, detail) => `<div class="tuile-rythme"><small>${titre}</small>`
    + `<b>${nb(valeur)}</b><span>${detail}</span></div>`;
  const tuiles = '<div class="tuiles-rythme">'
    + tuile('Tour le plus fort', fort.total, `tour ${fort.tour}`)
    + tuile('Tour le plus faible', faible.total, `tour ${faible.tour}`)
    + tuile(duree ? 'Sur la durée' : 'Moyenne', duree || moyenne, 'par tour')
    + '</div>';

  const m = { h: 18, b: 26, g: 46, d: 78 };
  const aire = aireDe(m);
  const ech = echelle(Math.max(...suite.map((d) => d.total), duree || 0));
  const y = (v) => m.h + aire.h - (v / ech.haut) * aire.h;
  const x = (i) => m.g + (suite.length > 1 ? (i / (suite.length - 1)) * aire.w : aire.w / 2);
  const lancers = suite.map((d) => d.total - d.debut);
  const avecDebut = suite.some((d) => d.debut > 0);
  const bande = (haut, bas) => `M${haut.map((v, i) => point(x(i), y(v))).join(' L')}`
    + ` L${bas.map((v, i) => point(x(i), y(v))).reverse().join(' L')} Z`;
  const zeros = suite.map(() => 0);
  const aires = `<path class="g-aire-1" d="${bande(lancers, zeros)}"/>`
    + (avecDebut
      ? `<path class="g-aire-2" d="${bande(suite.map((d) => d.total), lancers)}"/>`
        // 2 px de fond entre les deux couches.
        + `<path class="g-separation" d="M${lancers.map((v, i) => point(x(i), y(v))).join(' L')}"/>`
      : '');
  const points = suite.map((d, i) => `<circle class="g-pt g-pt-encre" cx="${x(i).toFixed(1)}" cy="${y(d.total).toFixed(1)}"`
    + ` r="4" tabindex="0"><title>Tour ${d.tour} · ${nb(d.total)} dégâts${d.debut ? `, dont ${nb(d.debut)} en début de tour` : ''}</title></circle>`).join('');
  // Le tour le plus fort et le plus faible portent leur valeur : l'amplitude se
  // lit sans survol.
  const reperes = [fort, faible].filter((d, i, tab) => tab.indexOf(d) === i)
    .map((d) => `<text class="g-repere" x="${x(suite.indexOf(d)).toFixed(1)}" y="${(y(d.total) - 9).toFixed(1)}"`
      + ` text-anchor="middle">${nb(d.total)}</text>`).join('');
  const axeX = suite.map((d, i) => `<text class="g-axe" x="${x(i).toFixed(1)}" y="${H - 8}" text-anchor="middle">${d.tour}</text>`).join('');
  const ligneDuree = duree
    ? `<line class="g-moyenne" x1="${m.g}" x2="${L - m.d + 4}" y1="${y(duree).toFixed(1)}" y2="${y(duree).toFixed(1)}"/>`
      + `<text class="g-moyenne-texte" x="${L - m.d + 10}" y="${(y(duree) + 3.5).toFixed(1)}">${nb(duree)} / tour</text>`
    : '';
  const legende = '<div class="g-legende">'
    + (avecDebut ? '<span><i class="g-puce g-p-1"></i>Lancé dans le tour</span>'
      + '<span><i class="g-puce g-p-2"></i>En début de tour : explosion différée, poison, invocations</span>' : '')
    + (duree ? '<span><i class="g-puce g-p-moyenne"></i>Sur la durée</span>' : '')
    + '</div>';
  return tuiles + enveloppe(
    cibles > 1 ? `Sur ${cibles} ennemis.` : '',
    fond(m, aire, ech) + aires + ligneDuree + points + reperes + axeX,
    legende,
  );
}

// ---------------------------------------------------------------------
// Les dégâts par PA
// ---------------------------------------------------------------------
// Ce que rapporte chaque point d'action dépensé dans un sort : ses dégâts sur la
// rotation, divisés par ses lancers, puis par son coût. Un point par sort, du plus
// rentable au moins rentable ; le calcul s'écrit à droite.
export function parPA(tours, repartition) {
  const stats = new Map();
  (tours || []).forEach((t) => t.casts.forEach((c) => {
    const s = stats.get(c.id) || { nom: c.name, lancers: 0, pa: c.ap };
    s.lancers += 1;
    stats.set(c.id, s);
  }));
  const degats = new Map((repartition || []).map((p) => [p.id, p.damage]));
  const lignes = [];
  const soutien = [];
  for (const [id, s] of stats) {
    if (!(s.pa > 0)) continue;
    const d = degats.get(id) || 0;
    if (d <= 0) {
      soutien.push(s.nom);
      continue;
    }
    const parLancer = d / s.lancers;
    lignes.push({ nom: s.nom, pa: s.pa, parLancer, ratio: parLancer / s.pa });
  }
  if (!lignes.length) return '';
  lignes.sort((a, b) => b.ratio - a.ratio);
  const h = 26;
  const haut = lignes.length * h + 34;
  const g = 160;
  const d = 200;
  const ech = echelle(lignes[0].ratio * 1.05);
  const bas = Math.max(0, Math.floor((lignes[lignes.length - 1].ratio * 0.85) / ech.pas) * ech.pas);
  const x = (v) => g + ((v - bas) / (ech.haut - bas)) * (L - g - d);
  let grille = '';
  for (let v = bas; v <= ech.haut + 0.001; v += ech.pas) {
    grille += `<line class="g-grille" x1="${x(v).toFixed(1)}" x2="${x(v).toFixed(1)}" y1="6" y2="${haut - 22}"/>`
      + `<text class="g-axe" x="${x(v).toFixed(1)}" y="${haut - 8}" text-anchor="middle">${nb(v)}</text>`;
  }
  const rangs = lignes.map((l, i) => {
    const yy = 18 + i * h;
    return `<text class="g-texte" x="0" y="${yy + 4}">${echapper(l.nom)}</text>`
      + `<circle class="g-pt g-pt-1" cx="${x(l.ratio).toFixed(1)}" cy="${yy}" r="5" tabindex="0">`
      + `<title>${echapper(l.nom)} · ${nb(l.ratio)} dégâts par PA</title></circle>`
      + `<text class="g-texte" x="${L}" y="${yy + 4}" text-anchor="end">${nb(l.parLancer)} ÷ ${l.pa} PA = ${nb(l.ratio)}</text>`;
  }).join('');
  const note = soutien.length
    ? `<p class="aide">Sans dégâts directs, ${soutien.length > 1 ? 'ils ne sont pas classés' : 'il n\'est pas classé'} : ${
      soutien.map(echapper).join(', ')}.</p>` : '';
  return `<figure class="graphique"><figcaption>Dégâts d'un lancer, divisés par son coût en PA.</figcaption>`
    + `<svg viewBox="0 0 ${L} ${haut}">${grille}${rangs}</svg>${note}</figure>`;
}

// ---------------------------------------------------------------------
// Deux builds : ce qui les sépare
// ---------------------------------------------------------------------
/// Combien de fois chaque sort est lancé dans une rotation.
function lancersDe(r) {
  const n = new Map();
  r.turns.forEach((t) => t.casts.forEach((c) => n.set(c.id, (n.get(c.id) || 0) + 1)));
  return n;
}

// Ce que chaque source ajoute ou retire de B à A, avec sa raison : lancée plus
// ou moins souvent, par un seul des deux, ou plus ou moins forte à chaque lancer.
// La somme des barres est l'écart total.
export function ecartParSort(ra, rb) {
  const la = lancersDe(ra);
  const lb = lancersDe(rb);
  const sources = new Map();
  [[ra, 'a'], [rb, 'b']].forEach(([r, cote]) => (r.repartition || []).forEach((p) => {
    const s = sources.get(p.id) || { nom: p.name, a: 0, b: 0 };
    s[cote] = p.damage;
    sources.set(p.id, s);
  }));
  const lignes = [...sources.entries()].map(([id, s]) => {
    const na = la.get(id) || 0;
    const nbB = lb.get(id) || 0;
    const delta = s.b - s.a;
    let raison;
    if (na && !nbB) raison = `${na} ${pluriel(na, 'lancer')} chez A seulement`;
    else if (!na && nbB) raison = `${nbB} ${pluriel(nbB, 'lancer')} chez B seulement`;
    else if (!na) raison = 'hors lancers';
    else if (nbB !== na) {
      const k = Math.abs(nbB - na);
      raison = `${k} ${pluriel(k, 'lancer')} ${nbB > na ? 'de plus' : 'de moins'}`;
    } else raison = delta > 0 ? 'plus fort par lancer' : 'moins fort par lancer';
    return { nom: s.nom, delta, raison };
  }).filter((l) => Math.abs(Math.round(l.delta)) >= 1)
    .sort((x, y) => Math.abs(y.delta) - Math.abs(x.delta)).slice(0, 12);
  if (!lignes.length) return '<p class="aide">Mêmes dégâts, sort par sort.</p>';
  const h = 34;
  const bas = 10 + lignes.length * h;
  const haut = bas + 44;
  const debut = 200;
  const fin = 560;
  const milieu = (debut + fin) / 2;
  const plusGrand = Math.max(...lignes.map((l) => Math.abs(l.delta)));
  const long = (v) => Math.max(2, (Math.abs(v) / plusGrand) * ((fin - debut) / 2));
  const signe = (v) => (v < 0 ? '−' : '+');
  const rangs = lignes.map((l, i) => {
    const yy = 10 + i * h;
    const w = long(l.delta);
    const xr = l.delta < 0 ? milieu - w : milieu;
    return `<text class="g-texte" x="0" y="${yy + 12}">${echapper(l.nom)}</text>`
      + `<text class="g-repere" x="0" y="${yy + 25}">${echapper(l.raison)}</text>`
      + `<rect class="${l.delta < 0 ? 'g-serie-1' : 'g-serie-2'}" x="${xr.toFixed(1)}" y="${yy + 6}" width="${w.toFixed(1)}"`
      + ` height="14" rx="3" tabindex="0"><title>${echapper(l.nom)} · ${signe(l.delta)}${nb(Math.abs(l.delta))}</title></rect>`
      + `<text class="g-texte" x="${L}" y="${yy + 17}" text-anchor="end">${signe(l.delta)}${nb(Math.abs(l.delta))}</text>`;
  }).join('');
  const total = rb.total - ra.total;
  return `<figure class="graphique"><svg viewBox="0 0 ${L} ${haut}">`
    + `<line class="g-zero" x1="${milieu}" x2="${milieu}" y1="4" y2="${bas}"/>${rangs}`
    + `<text class="g-repere" x="${milieu - 6}" y="${bas + 14}" text-anchor="end">← A devant</text>`
    + `<text class="g-repere" x="${milieu + 6}" y="${bas + 14}">B devant →</text>`
    + `<text class="g-texte g-total" x="${L}" y="${bas + 36}" text-anchor="end">Écart total : ${signe(total)}${nb(Math.abs(total))}</text>`
    + '</svg></figure>';
}

// Les dégâts cumulés de deux builds, tour après tour : on y lit qui mène, et
// quand l'autre passe devant.
export function cumulDeuxBuilds(a, b, nomA, nomB) {
  const n = Math.max(a.length, b.length);
  if (!n) return '';
  const cumul = (v) => {
    let s = 0;
    return v.map((x) => (s += x));
  };
  const ca = cumul(a);
  const cb = cumul(b);
  const m = { h: 18, b: 26, g: 46, d: 112 };
  const aire = aireDe(m);
  const ech = echelle(Math.max(ca[ca.length - 1] || 0, cb[cb.length - 1] || 0));
  const y = (v) => m.h + aire.h - (v / ech.haut) * aire.h;
  const x = (i) => m.g + (n > 1 ? (i / (n - 1)) * aire.w : aire.w / 2);
  const trace = (vals, k, nom) => `<path class="g-ligne-${k}" d="M${vals.map((v, i) => point(x(i), y(v))).join(' L')}"/>`
    + vals.map((v, i) => `<circle class="g-pt g-pt-${k}" cx="${x(i).toFixed(1)}" cy="${y(v).toFixed(1)}" r="4" tabindex="0">`
      + `<title>Tour ${i + 1} · ${echapper(nom)} · ${nb(v)} cumulés</title></circle>`).join('');
  // Les deux étiquettes de fin ne se chevauchent pas.
  let ya = y(ca[ca.length - 1]) + 4;
  let yb = y(cb[cb.length - 1]) + 4;
  if (Math.abs(ya - yb) < 13) {
    if (ya <= yb) yb = ya + 13; else ya = yb + 13;
  }
  const fins = `<text class="g-texte" x="${L - m.d + 10}" y="${ya.toFixed(1)}">A · ${nb(ca[ca.length - 1])}</text>`
    + `<text class="g-texte" x="${L - m.d + 10}" y="${yb.toFixed(1)}">B · ${nb(cb[cb.length - 1])}</text>`;
  // Les tours où mène celui qui finit derrière : le premier passage se dit.
  const devant = ca.map((v, i) => Math.sign(v - (cb[i] ?? v)));
  const final = devant[devant.length - 1];
  const autre = final > 0 ? 'B' : 'A';
  const menes = devant.map((s, i) => (final && s === -final ? i : -1)).filter((i) => i >= 0);
  let annonce = '';
  if (menes.length) {
    const premier = menes[0];
    let dernier = premier;
    while (menes.includes(dernier + 1)) dernier += 1;
    const texte = premier === dernier ? `${autre} mène au tour ${premier + 1}` : `${autre} mène des tours ${premier + 1} à ${dernier + 1}`;
    const ix = x((premier + dernier) / 2);
    const iy = Math.min(y(ca[premier]), y(cb[premier])) - 12;
    annonce = `<text class="g-repere" x="${ix.toFixed(1)}" y="${iy.toFixed(1)}" text-anchor="middle">${texte}</text>`;
  }
  const axeX = Array.from({ length: n }, (_, i) => `<text class="g-axe" x="${x(i).toFixed(1)}" y="${H - 8}" text-anchor="middle">${i + 1}</text>`).join('');
  const legende = '<div class="g-legende">'
    + `<span><i class="g-puce g-p-1"></i>A · ${echapper(nomA)}</span>`
    + `<span><i class="g-puce g-p-2"></i>B · ${echapper(nomB)}</span></div>`;
  return enveloppe('Dégâts cumulés, tour après tour.', fond(m, aire, ech) + trace(ca, 1, nomA) + trace(cb, 2, nomB)
    + fins + annonce + axeX, legende);
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
