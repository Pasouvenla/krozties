// La charpente : quel outil est à l'écran, et sur quel personnage. Elle ne
// calcule rien et ne dessine aucun outil : elle tient l'équipement courant,
// l'outil affiché et le pont vers le moteur ; chaque outil est un module qui
// reçoit cet état et rend son écran.

import { appeler, hote } from './pont.js';
import { echapper, nb } from './commun/format.js';
import { brancherInfobulles, ficheDeSort, ficheDObjet } from './commun/infobulle.js';
import { brancherDetails } from './commun/animation.js';
import { garderOuverture, lireOuverture } from './commun/ouverture.js';
import { chargerBuild, courant, lire, oublierCourant } from './commun/personnages.js';
import { dessinerSelecteur } from './commun/selecteur-perso.js';
import { lireUneReception, resumeDuPartage } from './commun/partage.js';
import { cochesDuLien } from './commun/lien.js';
import { sortsDuBuild } from './commun/deck.js';
import { reglagesRotation } from './commun/reglages-rotation.js';

const $ = (id) => document.getElementById(id);

/// L'état partagé. Un seul objet, passé aux outils, jamais recopié.
export const etat = {
  build: null,      // la charge importée, telle qu'elle est envoyée au moteur
  resolu: null,     // ce que le moteur en a résolu : stats, classe, panoplies
  classes: null,    // le catalogue des classes et de leurs sorts
  refusCatalogue: null, // ce que le moteur a répondu s'il a refusé
  outil: 'equipement',
  replies: { invocations: true }, // les encarts repliés, voir `commun/repli.js`
};

/// Les outils connus. Chargés à la demande : tant qu'on ne va pas dans le
/// réseau de pièges, son code n'est pas lu.
const OUTILS = {
  // Les personnages enregistrés et leurs builds.
  personnages: () => import('./outils/personnages.js'),
  equipement: () => import('./outils/equipement.js'),
  rotation: () => import('./outils/rotation.js'),
  // KrozZone (les zones de sorts) et KrozTrap (les pièges) vivent dans KrozTools,
  // chacun dans son onglet.
  kroztools: () => import('./outils/kroztools.js'),
  // Les changements que la bêta annonce, avant leur sortie.
  beta: () => import('./outils/beta.js'),
  // Un calcul faux ou une idée, vers le formulaire d'issue du dépôt.
  retours: () => import('./outils/retours.js'),
  // Ce que la rotation ne compte pas encore, à relever en jeu.
  manquantes: () => import('./outils/manquantes.js'),
  notes: () => import('./outils/notes.js'),
  comparaison: () => import('./outils/comparaison.js'),
};

/// Le catalogue des classes se charge une fois, au démarrage. C'est une promesse
/// qu'on attend, pas un champ rempli plus tard : dans un navigateur, il arrive
/// après le premier dessin.
const catalogue = appeler('classes')
  .then((c) => { etat.classes = c; })
  .catch((e) => { etat.classes = null; etat.refusCatalogue = e.message; });

let moduleCourant = null;

/// Un sort par son identifiant : dans la classe de l'équipement d'abord, celle
/// que la Rotation affiche, puis dans celle des Zones, puis partout.
function sortParId(id) {
  const classes = (etat.classes && etat.classes.classes) || [];
  const dans = (classeId) => ((classes.find((c) => c.id === classeId) || {}).spells || [])
    .find((s) => s.id === id);
  // L'arme du build et les sorts de ses objets ne sont dans aucune classe :
  // leur fiche vient de la résolution.
  if (id === 'arme') return (etat.resolu && etat.resolu.arme) || null;
  if (id.startsWith('sort_d_objet_')) {
    return ((etat.resolu && etat.resolu.sorts_d_objets) || []).find((s) => s.id === id) || null;
  }
  return dans(etat.resolu && etat.resolu.class) || dans(etat.zonesClasse)
    || classes.map((c) => (c.spells || []).find((s) => s.id === id)).find(Boolean) || null;
}

brancherInfobulles({
  sort: (id) => {
    const sort = sortParId(id);
    if (!sort) return null;
    const r = etat.resolu;
    const deLaClasse = r && (id === 'arme' || id.startsWith('sort_d_objet_')
      || (((etat.classes && etat.classes.classes) || [])
      .find((c) => c.id === r.class) || { spells: [] }).spells.some((s) => s.id === id));
    return ficheDeSort(sort, {
      critique: deLaClasse ? r.crit : undefined,
      tables: deLaClasse ? r.damage_tables : undefined,
      modifs: deLaClasse ? r.sorts_modifies : undefined,
      certain: etat.classes && etat.classes.limites && etat.classes.limites.critique_certain,
    });
  },
  // La fiche de l'objet qui occupe cet emplacement de l'équipement.
  objet: (emplacement) => {
    const objet = ((etat.resolu && etat.resolu.stuff) || []).find((e) => e.slot === emplacement);
    return objet && objet.name ? ficheDObjet(objet) : null;
  },
});

export async function afficher(nom) {
  if (!OUTILS[nom]) return;
  etat.outil = nom;
  // Pendant l'ouverture, Équipement attend le build : il ne remplace pas
  // l'outil retenu.
  if (!etat.ouverture) garderOuverture(etat);
  document.querySelectorAll('#rail button[data-outil]').forEach((b) => {
    b.toggleAttribute('aria-current', b.dataset.outil === nom);
    if (b.dataset.outil === nom) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  });
  const contenu = $('contenu');
  contenu.innerHTML = '<p class="aide" style="padding:1.2rem">Chargement…</p>';
  const [mod] = await Promise.all([OUTILS[nom](), catalogue]);
  moduleCourant = mod;
  await mod.rendre(contenu, etat, { appeler, majIdentite, afficher });
}

/// L'entête dit toujours sur quel personnage on travaille : le personnage et
/// le build enregistrés, ou la classe d'un build importé sans l'être.
export function majIdentite() {
  const r = etat.resolu;
  dessinerSelecteur($('selecteurPerso'), etat, { appeler, majIdentite, afficher });
  // Un build enregistré dit déjà qui l'on calcule, chargé ou en chargement.
  $('identite').hidden = Boolean(courant(lire()));
  if (!r) {
    $('identite').innerHTML = '<span class="vide">Aucun équipement chargé</span>';
    $('changerEquipement').hidden = true;
    verrouiller(true);
    return;
  }
  $('identite').textContent = `${r.class_name || 'Classe inconnue'} · niveau ${r.level || '?'}`;
  $('changerEquipement').hidden = false;
  verrouiller(false);
}

/// Les outils qui exigent un équipement se grisent tant qu'il manque. Seule la
/// Rotation en exige un ; les KrozTools calculent sinon sur les valeurs de base.
function verrouiller(sansBuild) {
  document.querySelectorAll('#rail button[data-exige-build]').forEach((b) => {
    b.disabled = sansBuild;
  });
  $('noteRail').textContent = sansBuild
    ? 'Chargez un équipement pour ouvrir la rotation.'
    : '';
}

$('rail').addEventListener('click', (e) => {
  const b = e.target.closest('button[data-outil]');
  if (b && !b.disabled) afficher(b.dataset.outil);
});
$('changerEquipement').addEventListener('click', () => afficher('equipement'));

$('hote').textContent = hote === 'bureau' ? '' : 'Navigateur';

// Les blocs dépliables s'ouvrent et se replient en glissant, partout.
brancherDetails(document);

// L'application rouvre sur l'outil et l'onglet de KrozTools quittés. Un outil
// qui exige un équipement attend le dernier build enregistré, sur Équipement.
const ouverture = lireOuverture();
const voulu = OUTILS[ouverture.outil] ? ouverture.outil : 'equipement';
if (ouverture.kroztool) etat.kroztool = ouverture.kroztool;
const exigeBuild = Boolean(document.querySelector(`#rail button[data-outil="${voulu}"][data-exige-build]`));
majIdentite();

// Le dernier build enregistré chargé l'est de nouveau à l'ouverture.
const dernier = courant(lire());
if (dernier && exigeBuild) {
  etat.ouverture = true;
  afficher('equipement');
} else {
  afficher(voulu);
}
const demarrage = !dernier ? catalogue : catalogue
    .then(() => chargerBuild(etat, dernier.perso, dernier.build, { appeler, majIdentite }))
    .then(() => {
      const attendu = etat.ouverture && etat.outil === 'equipement';
      etat.ouverture = false;
      return afficher(attendu ? voulu : etat.outil);
    })
    .catch(() => {
      etat.ouverture = false;
      oublierCourant();
      majIdentite();
    });

// Les nouveautés de la version qu'on vient d'installer, puis une version plus
// récente : seule l'application de bureau les cherche, et une lecture qui
// échoue ne gêne rien.
if (hote === 'bureau') {
  import('./commun/nouveautes.js')
    .then((m) => m.montrer(appeler, afficher))
    .catch(() => {})
    .then(() => import('./commun/mise-a-jour.js'))
    .then((m) => m.verifier())
    .catch(() => {});
}

// Une rotation reçue : un fichier `.krozties` ouvert d'un double-clic, déposé
// sur la fenêtre ou passé au lancement, ou un lien ouvert dans le système ou
// collé dans Équipement. Un bandeau sous l'entête propose de l'ouvrir : son
// build et ses réglages se chargent, puis elle se calcule.
let recue = null;

const classeDu = (id) => ((etat.classes && etat.classes.classes) || []).find((c) => c.id === id);

async function montrerLaRotationRecue(recu) {
  recue = null;
  let corps;
  try {
    if (recu.erreur) throw new Error(recu.erreur);
    const d = lireUneReception(recu);
    await catalogue;
    corps = `<span>${echapper(resumeDuPartage(d, (id) => (classeDu(id) || {}).label || `Classe ${id}`))}</span>`;
    recue = d;
  } catch (e) {
    corps = `<span class="alerte">${echapper(e.message)}</span>`;
  }
  $('rotationRecue').innerHTML = `<b>Rotation reçue</b>${corps}<span class="actions">${recue
    ? '<button type="button" class="action petit" data-recue="ouvrir">Ouvrir</button>' : ''}<button type="button"
    class="action discret petit" data-recue="ignorer">${recue ? 'Ignorer' : 'Fermer'}</button></span>`;
  $('rotationRecue').hidden = false;
}

async function ouvrirLaRotationRecue() {
  const d = recue;
  if (!d) return;
  $('rotationRecue').querySelector('[data-recue=ouvrir]').disabled = true;
  try {
    // Le dernier build enregistré, rechargé au démarrage, ne doit pas
    // remplacer celui qu'on ouvre.
    await demarrage.catch(() => {});
    const resolu = await appeler('resoudre', d.build);
    if (resolu.error) throw new Error(resolu.error);
    etat.build = d.build;
    etat.nomDuBuild = null;
    etat.lienDuBuild = null;
    etat.luLe = null;
    etat.ficheDofusbook = null;
    // Une rotation reçue n'est à aucun personnage tant qu'on ne l'a pas
    // enregistrée.
    oublierCourant();
    etat.resolu = resolu;
    // Les sorts cochés d'un lien, sur la liste que la Rotation propose pour ce
    // build ; leur défaut si elle a changé depuis.
    const reglages = { ...d.reglages };
    if (d.cochesDuLien) reglages.coches = cochesDuLien(d.cochesDuLien, sortsDuBuild(classeDu(d.build.class), resolu));
    etat.rotation = null;
    Object.assign(reglagesRotation(etat), reglages);
    etat.bonusEcartes = [...d.bonus.ecartes];
    etat.bonusInclus = [...d.bonus.inclus];
    // Le placement reçu devient celui des KrozTools, plateau compris.
    if (d.placement) {
      etat.placement = { lanceur: d.placement.lanceur, visee: d.placement.ennemis[0] || d.placement.lanceur,
        ennemis: d.placement.ennemis.map((c) => [...c]) };
      if (d.placement.plateau) etat.plateau = { carte: d.placement.plateau.carte ?? null, damier: d.placement.plateau.damier || 15 };
    }
    recue = null;
    $('rotationRecue').hidden = true;
    majIdentite();
    await afficher('rotation');
    if ($('calculer')) $('calculer').click();
  } catch (e) {
    montrerLaRotationRecue({ erreur: e.message });
  }
}

$('rotationRecue').addEventListener('click', (e) => {
  const b = e.target.closest('[data-recue]');
  if (!b) return;
  if (b.dataset.recue === 'ouvrir') {
    ouvrirLaRotationRecue();
  } else {
    recue = null;
    $('rotationRecue').hidden = true;
  }
});

// L'application de bureau garde la rotation reçue jusqu'à ce que la page la
// lise : au démarrage, puis chaque fois qu'elle en reçoit une.
window.rotationRecue = async () => {
  try {
    const r = await appeler('rotationRecue');
    if (r) montrerLaRotationRecue(r);
  } catch (e) { /* rien à proposer */ }
};
if (hote === 'bureau') window.rotationRecue();

// Dans un navigateur, le fichier se dépose sur la page ; l'application de
// bureau reçoit les fichiers déposés avant elle.
document.addEventListener('dragover', (e) => {
  if (e.dataTransfer && [...e.dataTransfer.types].includes('Files')) e.preventDefault();
});
document.addEventListener('drop', async (e) => {
  const fichier = [...((e.dataTransfer && e.dataTransfer.files) || [])].find((f) => f.name.endsWith('.krozties'));
  if (!fichier) return;
  e.preventDefault();
  montrerLaRotationRecue({ texte: await fichier.text() });
});

// Un lien collé dans Équipement.
document.addEventListener('krozties:lien', (e) => montrerLaRotationRecue({ lien: e.detail }));
