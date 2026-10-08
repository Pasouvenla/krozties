// La charpente : quel outil est à l'écran, et sur quel personnage. Elle ne
// calcule rien et ne dessine aucun outil : elle tient l'équipement courant,
// l'outil affiché et le pont vers le moteur ; chaque outil est un module qui
// reçoit cet état et rend son écran.

import { appeler, hote } from './pont.js';
import { nb } from './commun/format.js';
import { brancherInfobulles, ficheDeSort, ficheDObjet } from './commun/infobulle.js';

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
  equipement: () => import('./outils/equipement.js'),
  rotation: () => import('./outils/rotation.js'),
  // KrozZone (les zones de sorts) et KrozTrap (les pièges) vivent dans KrozTools,
  // chacun dans son onglet.
  kroztools: () => import('./outils/kroztools.js'),
  // Les changements que la bêta annonce, avant leur sortie.
  beta: () => import('./outils/beta.js'),
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

/// L'entête dit toujours sur quel personnage on travaille.
export function majIdentite() {
  const r = etat.resolu;
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

majIdentite();
afficher('equipement');

// Une nouvelle version : seule l'application de bureau la cherche, et une
// vérification qui échoue ne gêne rien.
if (hote === 'bureau') {
  import('./commun/mise-a-jour.js').then((m) => m.verifier()).catch(() => {});
}
