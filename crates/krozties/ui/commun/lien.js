// Le lien d'une rotation : le build réduit à ce que le calcul lit, et les
// réglages de la Rotation, écrits en binaire compact puis en base64url après le
// `#` d'une adresse. Ce qui suit le `#` ne quitte jamais le navigateur de celui
// qui ouvre le lien.
//
// L'en-tête vient d'abord, seul lu par la page du lien : version, classe,
// niveau, tours, ennemis, position. Puis le build, puis les réglages. Les sorts
// cochés s'écrivent en cases, dans l'ordre des sorts que la Rotation propose
// pour ce build, avec leur nombre et une empreinte : une liste changée par une
// mise à jour les fait reprendre leur défaut.

export const ADRESSE = 'https://pasouvenla.github.io/krozties/r/';
/// Le même lien, tel que le système le donne à l'application de bureau.
export const PROTOCOLE = 'krozties://r';
const VERSION = 1;
const EMPLACEMENTS = ['ch', 'am', 'a1', 'a2', 'ce', 'bo', 'ca', 'br', 'ar', 'fa', 'mo', 'd1', 'd2', 'd3', 'd4', 'd5', 'd6'];
const CARACTERISTIQUES = ['vitality', 'wisdom', 'strength', 'intelligence', 'chance', 'agility'];
const POSITIONS = ['libre', 'distance', 'melee'];
const MAITRISES = [300, 0, 360];
/// Les codes de caractéristique de la forgemagie et des bonus, et les bonus de
/// Dofus connus : un rang au lieu du texte. On n'y ajoute qu'à la fin, sans
/// rien déplacer, sinon les liens déjà donnés se liraient de travers ; un nom
/// absent s'écrit en toutes lettres.
const CODES = ['pa', 'pm', 'po', 'cc', 'dc', 'dd', 'deg', 'dmg', 'fo', 'in', 'ch', 'ag', 'sa', 'vi', 'pu', 'do', 'ds',
  'dm', 'dnf', 'dtf', 'dff', 'def', 'daf', 'dw', 'dp'];
const BONUS = ['Rêve Nébuleux', 'Harmonie de Pandala', 'Rouge Vermeil', 'Jaune Ocre', 'Bleu Turquoise', 'Pourpre Profond'];

class Ecrit {
  constructor() { this.octets = []; }

  entier(n) {
    let v = Math.max(0, Math.floor(Number(n) || 0));
    do {
      let b = v % 128;
      v = Math.floor(v / 128);
      if (v) b += 128;
      this.octets.push(b);
    } while (v);
  }

  signe(n) {
    const v = Math.round(Number(n) || 0);
    this.entier(v >= 0 ? v * 2 : -v * 2 - 1);
  }

  texte(s) {
    const o = new TextEncoder().encode(String(s ?? ''));
    this.entier(o.length);
    this.octets.push(...o);
  }

  /// Un nom pris dans une liste connue par son rang, sinon en toutes lettres.
  nom(s, liste) {
    const i = liste.indexOf(s);
    this.entier(i < 0 ? liste.length : i);
    if (i < 0) this.texte(s);
  }
}

class Lit {
  constructor(octets) {
    this.octets = octets;
    this.i = 0;
  }

  entier() {
    let v = 0;
    let poids = 1;
    let b;
    do {
      if (this.i >= this.octets.length) throw new Error('Ce lien est incomplet : copiez-le en entier.');
      b = this.octets[this.i++];
      v += (b % 128) * poids;
      poids *= 128;
    } while (b >= 128);
    return v;
  }

  signe() {
    const v = this.entier();
    return v % 2 ? -(v + 1) / 2 : v / 2;
  }

  octet() {
    if (this.i >= this.octets.length) throw new Error('Ce lien est incomplet : copiez-le en entier.');
    return this.octets[this.i++];
  }

  texte() {
    const n = this.entier();
    if (this.i + n > this.octets.length) throw new Error('Ce lien est incomplet : copiez-le en entier.');
    const o = this.octets.slice(this.i, this.i + n);
    this.i += n;
    return new TextDecoder().decode(new Uint8Array(o));
  }

  nom(liste) {
    const i = this.entier();
    return i < liste.length ? liste[i] : this.texte();
  }
}

const versBase64 = (octets) => {
  let binaire = '';
  for (const o of octets) binaire += String.fromCharCode(o);
  return btoa(binaire).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

const depuisBase64 = (texte) => {
  const binaire = atob(texte.replace(/-/g, '+').replace(/_/g, '/'));
  return [...binaire].map((c) => c.charCodeAt(0));
};

/// Une empreinte de la liste des sorts, pour reconnaître la même liste.
const empreinte = (sorts) => [...sorts.map((s) => s.id).join(',')]
  .reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 65521, 7);

/// Le lien d'une rotation : `essentiel`, le build tel que le calcul le lit
/// (`resolu.partage`) ; les réglages de la Rotation ; les sorts qu'elle propose
/// pour ce build, dans leur ordre ; les bonus de Dofus écartés ou inclus ; les
/// états que la classe déclare, dont seuls ceux qui quittent leur défaut
/// s'écrivent : la Rotation de celui qui reçoit remet les autres.
export function lienDuPartage(essentiel, reglages, sorts, bonus, declares) {
  const e = new Ecrit();
  e.entier(VERSION);
  e.entier(essentiel.class);
  e.entier(essentiel.level);
  e.entier(reglages.horizon);
  e.entier(reglages.cibles);
  e.entier(Math.max(0, POSITIONS.indexOf(reglages.position)));

  const objets = essentiel.items || [];
  e.entier(objets.length);
  objets.forEach((id) => e.entier(id));
  const investis = Object.entries(essentiel.invested || {}).filter(([, v]) => v);
  e.entier(investis.length);
  investis.forEach(([nom, v]) => { e.nom(nom, CARACTERISTIQUES); e.signe(v); });
  const forgemagie = Object.entries(essentiel.forgemagic || {})
    .flatMap(([emplacement, lignes]) => Object.entries(lignes || {}).filter(([, v]) => v).map(([stat, v]) => [emplacement, stat, v]));
  e.entier(forgemagie.length);
  forgemagie.forEach(([emplacement, stat, v]) => { e.nom(emplacement, EMPLACEMENTS); e.nom(stat, CODES); e.signe(v); });
  const totaux = Object.entries(essentiel.fmGlobal || {}).filter(([, v]) => v);
  e.entier(totaux.length);
  totaux.forEach(([stat, v]) => { e.nom(stat, CODES); e.signe(v); });
  e.texte(essentiel.fmWeapon || '');
  e.texte(essentiel.fmStealWeapon || '');
  const boosts = essentiel.boosts || [];
  e.entier(boosts.length);
  boosts.forEach((b) => {
    e.nom(b.name, BONUS);
    e.nom(b.stat, CODES);
    e.signe(b.percent);
    const morceaux = String(b.effect_id || '').split('-');
    const chiffre = Boolean(b.effect_id) && morceaux.every((m) => /^\d+$/.test(m));
    e.entier((b.active === true ? 1 : 0) | (b.active === false ? 2 : 0) | (b.class_id != null ? 4 : 0)
      | (chiffre ? 8 : 0) | (b.effect_id && !chiffre ? 16 : 0));
    if (b.class_id != null) e.entier(b.class_id);
    if (chiffre) {
      e.entier(morceaux.length);
      morceaux.forEach((m) => e.entier(Number(m)));
    } else if (b.effect_id) e.texte(b.effect_id);
  });

  const coches = reglages.coches instanceof Set ? reglages.coches : null;
  const resistances = reglages.resistances && reglages.resistances.mode !== 'aucune' ? reglages.resistances : null;
  const etats = (declares || [])
    .filter((d) => Number((reglages.etats || {})[d.id] || 0) !== (Number(d.defaut) || 0))
    .map((d) => [d.id, Number(reglages.etats[d.id])]);
  e.entier(reglages.pmDepenses || 0);
  e.entier(reglages.etalement || 0);
  e.entier(Math.max(0, MAITRISES.indexOf(reglages.maitrise)));
  e.entier((reglages.pousseesBloquees ? 1 : 0) | (reglages.exhaustif ? 2 : 0)
    | (reglages.invocationsDeClasse === false ? 4 : 0) | (reglages.invocationsCommunes ? 8 : 0)
    | (coches ? 16 : 0) | (resistances ? 32 : 0) | (etats.length ? 64 : 0) | (reglages.mur ? 128 : 0));
  if (coches) {
    e.entier(sorts.length);
    e.entier(empreinte(sorts));
    for (let i = 0; i < sorts.length; i += 8) {
      e.octets.push(sorts.slice(i, i + 8).reduce((o, s, k) => (coches.has(s.id) ? o | (1 << k) : o), 0));
    }
  }
  if (resistances) e.texte(JSON.stringify(resistances));
  if (etats.length) {
    e.entier(etats.length);
    etats.forEach(([id, v]) => { e.texte(id); e.signe(v); });
  }
  if (reglages.mur) e.texte(JSON.stringify(reglages.mur));
  for (const liste of [bonus.ecartes || [], bonus.inclus || []]) {
    e.entier(liste.length);
    liste.forEach((nom) => e.nom(nom, BONUS));
  }
  return `${ADRESSE}#${versBase64(e.octets)}`;
}

/// La charge d'un lien, adresse web ou protocole, ou `null` si ce n'en est
/// pas un.
export function chargeDuLien(texte) {
  const t = String(texte || '').trim();
  for (const prefixe of [`${ADRESSE}#`, `${PROTOCOLE}#`, `${PROTOCOLE}/#`]) {
    const debut = t.indexOf(prefixe);
    if (debut >= 0) return t.slice(debut + prefixe.length).split(/\s/)[0] || null;
  }
  return null;
}

/// L'en-tête seul : de quoi résumer la rotation sans lire le reste.
export function enTete(charge) {
  const l = new Lit(depuisBase64(charge));
  const version = l.entier();
  if (version > VERSION) throw new Error('Ce lien vient d\'une version plus récente : mettez Krozties à jour pour l\'ouvrir.');
  return { classe: l.entier(), niveau: l.entier(), tours: l.entier(), ennemis: l.entier(), position: POSITIONS[l.entier()] || 'libre', lecteur: l };
}

/// Un lien lu : le build, les réglages, et les sorts cochés en attente de la
/// liste que la Rotation proposera pour ce build (voir `cochesDuLien`).
export function lireLeLien(charge) {
  let tete;
  try {
    tete = enTete(charge);
  } catch (e) {
    if (/version plus récente|incomplet/.test(e.message)) throw e;
    throw new Error("Ce lien n'est pas celui d'une rotation Krozties.");
  }
  const l = tete.lecteur;
  const build = { class: tete.classe, level: tete.niveau, items: [], invested: {}, forgemagic: {}, fmGlobal: {}, boosts: [] };
  const nObjets = l.entier();
  for (let i = 0; i < nObjets; i++) build.items.push(l.entier());
  const nInvestis = l.entier();
  for (let i = 0; i < nInvestis; i++) build.invested[l.nom(CARACTERISTIQUES)] = l.signe();
  const nForgemagie = l.entier();
  for (let i = 0; i < nForgemagie; i++) {
    const emplacement = l.nom(EMPLACEMENTS);
    const stat = l.nom(CODES);
    (build.forgemagic[emplacement] = build.forgemagic[emplacement] || {})[stat] = l.signe();
  }
  const nTotaux = l.entier();
  for (let i = 0; i < nTotaux; i++) build.fmGlobal[l.nom(CODES)] = l.signe();
  build.fmWeapon = l.texte() || null;
  build.fmStealWeapon = l.texte() || null;
  const nBonus = l.entier();
  for (let i = 0; i < nBonus; i++) {
    const b = { name: l.nom(BONUS), stat: l.nom(CODES), percent: l.signe() };
    const d = l.entier();
    b.active = d & 1 ? true : d & 2 ? false : null;
    b.class_id = d & 4 ? l.entier() : null;
    if (d & 8) {
      const n = l.entier();
      const morceaux = [];
      for (let k = 0; k < n; k++) morceaux.push(l.entier());
      b.effect_id = morceaux.join('-');
    } else b.effect_id = d & 16 ? l.texte() : null;
    build.boosts.push(b);
  }

  const reglages = {
    horizon: tete.tours,
    cibles: tete.ennemis,
    position: tete.position,
    pmDepenses: l.entier(),
    etalement: l.entier(),
    maitrise: MAITRISES[l.entier()] ?? 300,
  };
  const d = l.entier();
  reglages.pousseesBloquees = Boolean(d & 1);
  reglages.exhaustif = Boolean(d & 2);
  reglages.invocationsDeClasse = !(d & 4);
  reglages.invocationsCommunes = Boolean(d & 8);
  let coches = null;
  if (d & 16) {
    const nombre = l.entier();
    const marque = l.entier();
    const cases = [];
    for (let i = 0; i < Math.ceil(nombre / 8); i++) cases.push(l.octet());
    coches = { nombre, empreinte: marque, cases };
  }
  if (d & 32) reglages.resistances = JSON.parse(l.texte());
  reglages.etats = {};
  if (d & 64) {
    const n = l.entier();
    for (let i = 0; i < n; i++) reglages.etats[l.texte()] = l.signe();
  }
  reglages.mur = d & 128 ? JSON.parse(l.texte()) : null;
  const bonus = { ecartes: [], inclus: [] };
  for (const liste of [bonus.ecartes, bonus.inclus]) {
    const n = l.entier();
    for (let i = 0; i < n; i++) liste.push(l.nom(BONUS));
  }
  return {
    build,
    reglages,
    bonus,
    coches,
    resume: { classe: tete.classe, niveau: tete.niveau, tours: tete.tours, ennemis: tete.ennemis, position: tete.position },
  };
}

/// Les sorts cochés d'un lien, sur la liste que la Rotation propose pour le
/// build reçu ; `null`, leur défaut, si la liste n'est plus la même.
export function cochesDuLien(coches, sorts) {
  if (!coches || coches.nombre !== sorts.length || coches.empreinte !== empreinte(sorts)) return null;
  return new Set(sorts.filter((s, i) => coches.cases[Math.floor(i / 8)] & (1 << (i % 8))).map((s) => s.id));
}
