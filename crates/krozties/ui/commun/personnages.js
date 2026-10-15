// Les personnages et leurs builds, gardés sur cet ordinateur. Un personnage a
// un nom, une classe et un niveau ; un build, son nom, son lien DofusBook, la
// charge importée et la fiche DofusBook qui la complète. `courant` dit quel
// build enregistré est chargé, pour le recharger à l'ouverture suivante.

const CLE = 'krozties.personnages';

/// Le nom d'une classe, en attendant le catalogue qui arrive après le premier
/// dessin.
export const CLASSES = {
  1: 'Féca', 2: 'Osamodas', 3: 'Enutrof', 4: 'Sram', 5: 'Xélor', 6: 'Ecaflip', 7: 'Eniripsa', 8: 'Iop',
  9: 'Crâ', 10: 'Sadida', 11: 'Sacrieur', 12: 'Pandawa', 13: 'Roublard', 14: 'Zobal', 15: 'Steamer',
  16: 'Eliotrope', 17: 'Huppermage', 18: 'Ouginak', 20: 'Forgelance',
};

/// Le badge d'une classe, par son identifiant.
export const ABREGES = {
  1: 'Féc', 2: 'Osa', 3: 'Enu', 4: 'Sra', 5: 'Xél', 6: 'Éca', 7: 'Eni', 8: 'Iop', 9: 'Crâ', 10: 'Sad',
  11: 'Sac', 12: 'Pan', 13: 'Rou', 14: 'Zob', 15: 'Ste', 16: 'Éli', 17: 'Hup', 18: 'Oug', 20: 'For',
};

const vide = () => ({ personnages: [], courant: null });

export function lire() {
  try {
    const d = JSON.parse(localStorage.getItem(CLE));
    if (d && Array.isArray(d.personnages)) return d;
  } catch { /* stockage refusé ou illisible : on repart de rien */ }
  return vide();
}

/// `false` quand le stockage refuse l'écriture (navigation privée, quota).
export function ecrire(d) {
  try {
    localStorage.setItem(CLE, JSON.stringify(d));
    return true;
  } catch {
    return false;
  }
}

const nouvelId = () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;

export function creerPersonnage(d, { nom, classe, niveau }) {
  const p = { id: nouvelId(), nom, classe, niveau, builds: [] };
  d.personnages.push(p);
  return p;
}

export function ajouterBuild(p, { nom, lien, charge, dofusbook, elements }) {
  const b = { id: nouvelId(), nom, lien: lien || null, charge, dofusbook: dofusbook || null, elements: elements || [] };
  p.builds.push(b);
  return b;
}

export function personnage(d, id) {
  return d.personnages.find((p) => p.id === id) || null;
}

/// Le personnage et le build enregistrés qui sont chargés, s'il y en a.
export function courant(d) {
  const p = d.courant && personnage(d, d.courant.perso);
  const b = p && p.builds.find((x) => x.id === d.courant.build);
  return b ? { perso: p, build: b } : null;
}

/// Les éléments d'un build résolu : ceux dont la caractéristique atteint les
/// trois quarts de la plus haute, hors Neutre.
export function elementsDe(resolu) {
  const els = ((resolu && resolu.elements) || []).filter((e) => e.name !== 'Neutre');
  const haut = Math.max(0, ...els.map((e) => e.characteristic));
  return haut > 0 ? els.filter((e) => e.characteristic >= haut * 0.75).map((e) => e.name) : [];
}

/// Charge un build enregistré comme le ferait un import : la fiche, la
/// Rotation et les KrozTools le voient aussitôt.
export async function chargerBuild(etat, p, b, { appeler, majIdentite }) {
  const resolu = await appeler('resoudre', b.dofusbook ? { ...b.charge, dofusbook: b.dofusbook } : b.charge);
  if (resolu.error) throw new Error(resolu.error);
  etat.build = b.charge;
  etat.resolu = resolu;
  etat.nomDuBuild = b.nom;
  etat.lienDuBuild = b.lien;
  etat.ficheDofusbook = b.dofusbook;
  const d = lire();
  d.courant = { perso: p.id, build: b.id };
  p.dernier = b.id;
  const enregistre = personnage(d, p.id);
  if (enregistre) enregistre.dernier = b.id;
  ecrire(d);
  majIdentite();
}

/// Un build importé sans être enregistré remplace le build courant.
export function oublierCourant() {
  const d = lire();
  if (d.courant) {
    d.courant = null;
    ecrire(d);
  }
}

/// La liste exportée : les personnages seuls, sans ce qui est chargé.
export function exporter(d) {
  return JSON.stringify({ format: 'krozties-personnages', version: 1, personnages: d.personnages }, null, 1);
}

/// Une liste exportée, fusionnée : un personnage déjà présent (même
/// identifiant) est remplacé, les autres s'ajoutent. Rend le nombre lu.
export function importer(d, texte) {
  const lu = JSON.parse(texte);
  const liste = lu && lu.format === 'krozties-personnages' && Array.isArray(lu.personnages) ? lu.personnages : null;
  if (!liste) throw new Error("ce fichier n'est pas une liste de personnages exportée par Krozties.");
  const valides = liste.filter((p) => p && p.id && typeof p.nom === 'string' && Number.isInteger(p.classe)
    && Array.isArray(p.builds));
  for (const p of valides) {
    const i = d.personnages.findIndex((x) => x.id === p.id);
    if (i >= 0) d.personnages[i] = p; else d.personnages.push(p);
  }
  return valides.length;
}
