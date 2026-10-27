// Un résultat mis en forme pour Discord : un clic, puis on colle dans le
// salon. Les chiffres vont dans un bloc à chasse fixe, où leurs colonnes
// s'alignent et où la répartition se lit en barres.

import { hote } from '../pont.js';
import { nb, pluriel } from './format.js';
import { decrire } from './resistances.js';

/// Ce qu'un message Discord accepte sans abonnement.
const LIMITE = 2000;
const POSITIONS = { libre: 'position au choix', distance: 'à distance', melee: 'en mêlée' };
const pourcent = (v) => `${(v * 100).toFixed(1).replace('.', ',')} %`;
/// Dans un bloc à chasse fixe, l'espace fine du français n'a pas la largeur
/// d'un chiffre et décalerait les colonnes.
const nbFixe = (v) => nb(v).replace(/[  ]/g, ' ');
const BLOC = '```';
/// La plus longue barre de la répartition, en caractères.
const BARRE = 16;
/// Un nom de sort plus long se coupe, pour garder les barres alignées.
const NOM = 18;

export async function version(appeler) {
  if (hote !== 'bureau') return null;
  try { return await appeler('version'); } catch (e) { return null; }
}

/// Les réglages d'une demande de rotation, en une ligne.
export function reglages(demande) {
  const tours = demande.horizon || 7;
  const ennemis = demande.targets || 1;
  const contre = decrire(demande);
  return [
    `${tours} ${pluriel(tours, 'tour')}`,
    `${ennemis} ${pluriel(ennemis, 'ennemi')}`,
    POSITIONS[demande.position] || POSITIONS.libre,
    contre ? `contre ${contre}` : 'cible sans résistance',
  ].join(' · ');
}

const sousTitre = (demande, v) => `-# ${reglages(demande)}${v ? ` · Krozties ${v}` : ''}`;

/// Les lancers d'un tour dans leur ordre, deux lancers de suite du même sort
/// réunis.
function sequence(tour) {
  const noms = [];
  for (const c of tour.casts) {
    const dernier = noms[noms.length - 1];
    if (dernier && dernier.nom === c.name) dernier.fois += 1;
    else noms.push({ nom: c.name, fois: 1 });
  }
  return noms.map((n) => (n.fois > 1 ? `${n.nom} ×${n.fois}` : n.nom)).join(' › ') || 'rien';
}

/// Les tours, ceux qui se suivent à l'identique réunis.
function groupes(r) {
  const liste = [];
  r.turns.forEach((t, i) => {
    const s = sequence(t);
    const g = liste[liste.length - 1];
    if (g && g.s === s) {
      g.fin = i + 1;
      g.degats.push(t.damage);
    } else {
      liste.push({ s, debut: i + 1, fin: i + 1, degats: [t.damage] });
    }
  });
  return liste;
}

/// Un tableau à chasse fixe : la première colonne à gauche, les autres à
/// droite ; la dernière, laissée libre, peut être longue.
function tableau(lignes, libre = false) {
  const n = lignes[0].length;
  const largeurs = [...Array(n).keys()].map((k) => Math.max(...lignes.map((l) => l[k].length)));
  return lignes.map((l) => l.map((c, k) => {
    if (k === 0) return c.padEnd(largeurs[k]);
    if (libre && k === n - 1) return c;
    return c.padStart(largeurs[k]);
  }).join('  ').trimEnd());
}

const coupe = (n) => (n.length > NOM ? `${n.slice(0, NOM - 1)}…` : n);

/// Les barres de la répartition, la plus grosse part pleine.
function barres(parts) {
  if (!parts.length) return [];
  const haut = Math.max(...parts.map((p) => p.share));
  const largeur = Math.max(...parts.map((p) => coupe(p.name).length));
  return parts.map((p) => `${coupe(p.name).padEnd(largeur)}  ${
    '█'.repeat(Math.max(1, Math.round((p.share / haut) * BARRE))).padEnd(BARRE)}  ${pourcent(p.share).padStart(6)}`);
}

/// Le message, coupé sous la limite de Discord : les dernières lignes du bloc
/// partent d'abord, remplacées par « … », et le bloc reste fermé.
function message(avant, bloc, apres) {
  const assembler = (lignes) => [...avant, BLOC, ...lignes, BLOC, ...apres].join('\n');
  let lignes = bloc;
  while (assembler(lignes).length > LIMITE && lignes.length > 1) {
    lignes = [...lignes.slice(0, lignes[lignes.length - 1] === '…' ? -2 : -1), '…'];
  }
  return assembler(lignes);
}

/// Le pied du message : la rotation à ouvrir dans Krozties, puis le build sur
/// DofusBook, chacun derrière son nom. Sous le bloc, la coupe ne les touche pas.
const pied = (partage, dofusbook) => {
  const liens = [
    partage ? `[Ouvrir dans Krozties](<${partage}>)` : null,
    dofusbook ? `[Le build sur DofusBook](<${dofusbook}>)` : null,
  ].filter(Boolean);
  return liens.length ? [`-# ${liens.join(' · ')}`] : [];
};

/// Une rotation : le build et son total, les réglages, puis les tours et la
/// répartition dans un bloc, et les liens de la rotation et du build.
export function rotation({ r, demande, build, version: v, lien, partage }) {
  const entete = `**${build}** · **${nb(r.total)} dégâts** · ${nb(r.per_turn)} par tour${
    r.steady && r.steady.per_turn ? ` · ${nb(r.steady.per_turn)} sur la durée` : ''}`;
  const tours = tableau([
    ['Tour', 'Dégâts', 'Sorts'],
    ...groupes(r).map((g) => [
      g.debut === g.fin ? `${g.debut}` : `${g.debut}-${g.fin}`,
      g.degats.length > 1 ? `≈ ${nbFixe(g.degats.reduce((a, b) => a + b, 0) / g.degats.length)}` : nbFixe(g.degats[0]),
      g.s,
    ]),
  ], true);
  const parts = barres((r.repartition || []).filter((p) => p.share >= 0.005));
  return message(
    [entete, sousTitre(demande, v)],
    parts.length ? [...tours, '', ...parts] : tours,
    pied(partage, lien),
  );
}

const ecartEnPourcent = (a, b) => (a ? ((b - a) / a) * 100 : 0);
const signe = (x, moins = '-') => (x < 0 ? moins : '+');

/// Une comparaison : l'écart de B sur A, les réglages, les deux builds, puis
/// les totaux et les sources qui font la différence dans un bloc.
export function comparaison({ ra, rb, demande, titre, nomA, nomB, version: v }) {
  const ecart = ecartEnPourcent(ra.total, rb.total);
  const enPourcent = (x) => `${signe(x)}${Math.abs(x).toFixed(1).replace('.', ',')} %`;
  const sources = new Map();
  [[ra, 'a'], [rb, 'b']].forEach(([r, cote]) => (r.repartition || []).forEach((p) => {
    const s = sources.get(p.id) || { nom: p.name, a: 0, b: 0 };
    s[cote] = Math.round(p.damage);
    sources.set(p.id, s);
  }));
  const differences = [...sources.values()]
    .filter((s) => s.a !== s.b)
    .sort((x, y) => Math.abs(y.b - y.a) - Math.abs(x.b - x.a))
    .slice(0, 8)
    .map((s) => [coupe(s.nom), nbFixe(s.a), nbFixe(s.b), `${signe(s.b - s.a)}${nbFixe(Math.abs(s.b - s.a))}`]);
  // Un seul tableau : les colonnes des totaux et des sorts s'alignent.
  const lignes = tableau([
    ['', 'A', 'B', 'Écart'],
    ['Total', nbFixe(ra.total), nbFixe(rb.total), enPourcent(ecart)],
    ['Par tour', nbFixe(ra.per_turn), nbFixe(rb.per_turn), enPourcent(ecartEnPourcent(ra.per_turn, rb.per_turn))],
    ...(differences.length ? [['', '', '', ''], ...differences] : []),
  ]);
  return message(
    [
      `**${titre}** · B **${signe(ecart, '−')}${Math.abs(ecart).toFixed(1).replace('.', ',')} %** sur A`,
      sousTitre(demande, v),
      `**A** · ${nomA}`,
      `**B** · ${nomB}`,
    ],
    lignes,
    [],
  );
}

/// Le texte dans le presse-papiers ; `false` si le système le refuse.
export async function copier(texte) {
  try {
    await navigator.clipboard.writeText(texte);
    return true;
  } catch (e) {
    const zone = document.createElement('textarea');
    zone.value = texte;
    zone.setAttribute('readonly', '');
    zone.style.position = 'fixed';
    zone.style.opacity = '0';
    document.body.appendChild(zone);
    zone.select();
    const ok = document.execCommand('copy');
    zone.remove();
    return ok;
  }
}
