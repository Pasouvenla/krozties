// Un résultat mis en forme pour Discord : un clic, puis on colle dans le
// salon.

import { hote } from '../pont.js';
import { nb, pluriel } from './format.js';
import { decrire } from './resistances.js';

/// Ce qu'un message Discord accepte sans abonnement.
const LIMITE = 2000;
const POSITIONS = { libre: 'au choix', distance: 'à distance', melee: 'en mêlée' };
const pourcent = (v) => `${(v * 100).toFixed(1).replace('.', ',')} %`;

export async function version(appeler) {
  if (hote !== 'bureau') return null;
  try { return await appeler('version'); } catch (e) { return null; }
}

/// Les réglages d'une demande de rotation, en une ligne.
export function reglages(demande) {
  const tours = demande.horizon || 7;
  const ennemis = demande.targets || 1;
  const contre = decrire(demande);
  return `${tours} ${pluriel(tours, 'tour')}, ${ennemis} ${pluriel(ennemis, 'ennemi')}, position ${
    POSITIONS[demande.position] || POSITIONS.libre}, ${contre ? `contre ${contre}` : 'cible sans résistance'}`;
}

const enTete = (titre, v) => `**${titre}**${v ? ` (Krozties ${v})` : ''}`;

/// Les lancers d'un tour, deux lancers de suite du même sort réunis.
function sequence(tour) {
  const noms = [];
  for (const c of tour.casts) {
    const dernier = noms[noms.length - 1];
    if (dernier && dernier.nom === c.name) dernier.fois += 1;
    else noms.push({ nom: c.name, fois: 1 });
  }
  return noms.map((n) => (n.fois > 1 ? `${n.nom} ×${n.fois}` : n.nom)).join(', ') || 'rien';
}

/// Les tours, ceux qui se suivent à l'identique réunis.
function tours(r) {
  const groupes = [];
  r.turns.forEach((t, i) => {
    const s = sequence(t);
    const g = groupes[groupes.length - 1];
    if (g && g.s === s) {
      g.fin = i + 1;
      g.degats.push(t.damage);
    } else {
      groupes.push({ s, debut: i + 1, fin: i + 1, degats: [t.damage] });
    }
  });
  return groupes.map((g) => (g.debut === g.fin
    ? `Tour ${g.debut} : ${g.s} (${nb(g.degats[0])})`
    : `Tours ${g.debut} à ${g.fin} : ${g.s} (≈ ${nb(g.degats.reduce((a, b) => a + b, 0) / g.degats.length)} par tour)`));
}

/// Coupe un message trop long pour Discord, ligne entière par ligne entière.
function borne(lignes) {
  const garde = [];
  let n = 0;
  for (const l of lignes) {
    if (n + l.length + 1 > LIMITE - 2) {
      garde.push('…');
      break;
    }
    garde.push(l);
    n += l.length + 1;
  }
  return garde.join('\n');
}

/// Une rotation : le build, les réglages, le total, les tours, la répartition
/// et le lien DofusBook.
export function rotation({ r, demande, build, version: v, lien }) {
  const lignes = [
    enTete(build, v),
    reglages(demande),
    `**${nb(r.total)}** dégâts · ${nb(r.per_turn)} par tour${
      r.steady && r.steady.per_turn ? ` · ${nb(r.steady.per_turn)} sur la durée` : ''}`,
    '',
    ...tours(r),
  ];
  const parts = (r.repartition || []).filter((p) => p.share >= 0.005);
  if (parts.length) lignes.push('', `Répartition : ${parts.map((p) => `${p.name} ${pourcent(p.share)}`).join(' · ')}`);
  if (lien) lignes.push(lien);
  return borne(lignes);
}

/// Une comparaison : A et B, l'écart, et les sources qui font la différence.
export function comparaison({ ra, rb, demande, titre, nomA, nomB, version: v }) {
  const ecart = ra.total ? ((rb.total - ra.total) / ra.total) * 100 : 0;
  const signe = ecart < 0 ? '−' : '+';
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
    .map((s) => `${s.nom} : ${nb(s.a)} → ${nb(s.b)}`);
  return borne([
    enTete(titre, v),
    reglages(demande),
    `A · ${nomA} : **${nb(ra.total)}** (${nb(ra.per_turn)} par tour)`,
    `B · ${nomB} : **${nb(rb.total)}** (${nb(rb.per_turn)} par tour), **${signe}${
      Math.abs(ecart).toFixed(1).replace('.', ',')} %**`,
    ...(differences.length ? ['', ...differences] : []),
  ]);
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
