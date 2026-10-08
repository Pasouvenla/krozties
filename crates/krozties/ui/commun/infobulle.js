// Les infobulles : celle d'un sort, et le texte de tout élément qui en porte un.
// Le `title` natif ne s'affiche pas dans la fenêtre de bureau (WebKit de
// Tauri) : la bulle est dessinée par la page. Un `title` existant est repris au
// premier survol ; `data-sort` reçoit la fiche du sort, `data-objet` celle de
// l'objet de l'emplacement.

import { echapper, FR_ELEMENT, majuscule, nb } from './format.js';

let bulle = null;

/// `sort(id)` rend la fiche d'un sort en HTML, ou rien si le sort est inconnu ;
/// `objet(emplacement)` celle de l'objet qui l'occupe.
export function brancherInfobulles({ sort, objet }) {
  if (bulle) return;
  bulle = document.createElement('div');
  bulle.className = 'infobulle';
  bulle.setAttribute('role', 'tooltip');
  bulle.hidden = true;
  document.body.appendChild(bulle);

  const cibleDe = (el) => (el && el.closest
    ? el.closest('[data-sort], [data-objet], [data-infobulle], [title]') : null);

  const montrer = (cible) => {
    if (cible.hasAttribute('title')) {
      const texte = cible.getAttribute('title');
      cible.removeAttribute('title');
      if (texte && !cible.dataset.infobulle) cible.dataset.infobulle = texte;
    }
    let html = null;
    if (cible.dataset.sort) html = sort(cible.dataset.sort);
    if (!html && cible.dataset.objet && objet) html = objet(cible.dataset.objet);
    if (!html && cible.dataset.infobulle) {
      html = echapper(cible.dataset.infobulle).replace(/\n/g, '<br>');
    }
    if (!html) { cacher(); return; }
    bulle.innerHTML = html;
    bulle.hidden = false;
    placer(cible);
  };

  // Elle ne doit pas gêner le clic : elle attend que la souris s'arrête, se pose
  // à côté, et part au premier clic.
  let attente = null;
  const cacher = () => {
    clearTimeout(attente);
    bulle.hidden = true;
  };

  document.addEventListener('mouseover', (e) => {
    const cible = cibleDe(e.target);
    clearTimeout(attente);
    if (!cible) { cacher(); return; }
    attente = setTimeout(() => montrer(cible), 450);
  });
  document.addEventListener('mousedown', cacher, true);
  document.addEventListener('focusin', (e) => {
    const cible = cibleDe(e.target);
    if (cible && e.target.matches(':focus-visible')) montrer(cible);
  });
  document.addEventListener('focusout', cacher);
  window.addEventListener('scroll', cacher, true);
}

/// À droite de l'élément si la place existe, à gauche sinon, dessous en
/// dernier recours : jamais sur les éléments voisins d'une liste, et jamais
/// hors de la fenêtre.
function placer(cible) {
  const c = cible.getBoundingClientRect();
  const b = bulle.getBoundingClientRect();
  const marge = 12;
  let x;
  let y = c.top;
  if (c.right + marge + b.width <= window.innerWidth - marge) x = c.right + marge;
  else if (c.left - marge - b.width >= marge) x = c.left - marge - b.width;
  else {
    x = Math.min(c.left, window.innerWidth - b.width - marge);
    y = c.bottom + marge;
  }
  if (y + b.height > window.innerHeight - marge) y = window.innerHeight - b.height - marge;
  bulle.style.left = `${Math.max(marge, x)}px`;
  bulle.style.top = `${Math.max(marge, y)}px`;
}

// ---------------------------------------------------------------------
// La fiche d'un sort
// ---------------------------------------------------------------------

/// Le taux de critique sur le build importé, avec son calcul à côté : le vrai
/// taux, même au-delà du seuil du critique sûr, que l'onglet Rotation explique.
function tauxCritique(sort, critique) {
  if (!sort.can_crit) return { texte: 'jamais', calcul: '' };
  if (sort.crit_base == null) return { texte: 'inconnu', calcul: '' };
  // Le critique qu'un objet de classe donne à ce sort : « Lapement : +35 %
  // Critique ». Il s'ajoute au taux de base, avant celui du build.
  const objets = sort.crit_objets || 0;
  const deObjet = objets ? ` + ${objets} de l'objet de classe` : '';
  // Le taux d'une arme est le sien, pas celui d'un sort.
  const base = sort.arme ? "de l'arme" : 'de base';
  if (typeof critique !== 'number') {
    return { texte: `${sort.crit_base + objets} %`, calcul: objets ? `${sort.crit_base} % ${base}${deObjet}` : base };
  }
  const brut = sort.crit_base + objets + critique;
  const effectif = Math.max(0, Math.min(100, brut));
  const plafonne = brut > 100 ? ', plafonné' : '';
  return {
    texte: `${effectif} %`,
    calcul: critique || objets
      ? `${sort.crit_base} % ${base}${deObjet}${critique ? ` + ${critique} du build` : ''}${plafonne}`
      : base,
    effectif,
  };
}

/// Ce qu'on sait d'un sort, avec ses dégâts sur le build importé s'il y en a un.
/// `critique` : le critique du build ; `tables` : les dégâts du build par sort ;
/// `certain` : le seuil du critique sûr ; `modifs` : ce que les objets de classe
/// changent aux sorts.
export function ficheDeSort(sortDuCatalogue, { critique, tables, certain, modifs } = {}) {
  const modif = (modifs && modifs[sortDuCatalogue.id]) || null;
  const sort = modif ? { ...sortDuCatalogue, ...modif } : sortDuCatalogue;
  const uniques = [...new Set((sort.elements || []).map((e) => FR_ELEMENT[e] || e))];
  const portee = sort.range
    ? (sort.range[0] === sort.range[1] ? `portée ${sort.range[0]}`
      : `portée ${sort.range[0]} à ${sort.range[1]}`)
    : null;
  const crit = tauxCritique(sort, critique);

  // Seul ce qui SORT de l'ordinaire est dit : tous les sorts exigent une
  // ligne de vue et acceptent un bonus de portée.
  const c = sort.cast || {};
  const contraintes = [];
  if (c.in_line && c.in_diagonal) contraintes.push('en ligne ou en diagonale');
  else if (c.in_line) contraintes.push('en ligne uniquement');
  else if (c.in_diagonal) contraintes.push('en diagonale uniquement');
  if (c.needs_line_of_sight === false) contraintes.push('sans ligne de vue');
  if (c.range_boostable === false) contraintes.push('portée non modifiable');
  if (c.needs_free_cell) contraintes.push('case libre exigée');
  if (c.needs_taken_cell) contraintes.push('case occupée exigée');
  if (c.needs_cell_without_portal) contraintes.push('case sans portail');
  if (c.needs_free_trap_cell) contraintes.push('case sans piège');

  // Les effets dont la valeur change sur un coup critique : une seule valeur, la
  // critique à partir du seuil du critique sûr (78 %), la normale en dessous ; un
  // effet qui vaut autant en critique ne se montre pas.
  const critEffets = (sort.critical_effects || []).filter((e) => e.normal !== e.critical).map((e) => {
    const applique = crit.effectif != null && certain != null && crit.effectif >= certain;
    return {
      // `suite` dit ce qui multiplie un bonus de dégâts de base : « par PM
      // utilisé », « avec Psychopathe ».
      texte: `${e.label} ${applique ? e.critical : e.normal}${e.suite ? ` ${e.suite}` : ''}`,
      autre: applique ? e.normal : e.critical,
      applique,
    };
  });

  const entete = [uniques.join(', ') || 'Sans élément', `${sort.ap} PA`, portee]
    .filter(Boolean).join(' · ');

  // Les dégâts sur le build importé, une ligne par palier de charge ; sans build,
  // le jet de base.
  const table = (tables && tables[sort.id]) || null;
  const etiquetteCharge = (n, total) => {
    if (total <= 1) return 'Dégâts';
    if (n === 0) return 'Sans charge';
    return `${n} ${n > 1 ? 'charges' : 'charge'}`;
  };
  const fourchette = (n, cc) => `${nb(n[0])} - ${nb(n[1])}`
    + (cc ? `<span class="calcul"> · ${nb(cc[0])} - ${nb(cc[1])} CC</span>` : '');
  // Un sort à plusieurs éléments se détaille, sur le DERNIER palier seulement :
  // ventiler les sept paliers du Glas donnait trente-cinq lignes.
  const ventiler = (r, dernier) => ((dernier && (r.by_element || []).length > 1)
    ? `<dd class="ventilation">${r.by_element.map((e) =>
      `${echapper(FR_ELEMENT[e.element] || e.element)} ${fourchette(e.normal, e.critical)}`,
    ).join('<br>')}</dd>`
    : '');
  const lignes = table
    ? table.map((r, i) => `<dt>${echapper(etiquetteCharge(r.charges, table.length))}</dt>`
      + `<dd>${fourchette(r.normal, r.critical)}</dd>${ventiler(r, i === table.length - 1)}`).join('')
    : (sort.lines || []).filter((l) => l.normal || l.critical).map((l) => {
      const n = l.normal ? `${l.normal[0]} - ${l.normal[1]}` : '?';
      const cc = l.critical ? `${l.critical[0]} - ${l.critical[1]}` : '?';
      const element = FR_ELEMENT[l.element]
        || { best: 'Meilleur élément', worst: 'Pire élément' }[l.element] || l.element;
      return `<dt>${echapper(element)}</dt>`
        + `<dd>${n}<span class="calcul"> · ${cc} CC</span></dd>`;
    }).join('');

  const limites = [];
  if (sort.casts_per_turn > 0) limites.push(`${sort.casts_per_turn} par tour`);
  if (sort.casts_per_target > 0) limites.push(`${sort.casts_per_target} par cible`);
  if (sort.cooldown === 1) limites.push('relance au tour suivant');
  else if (sort.cooldown > 1) limites.push(`relance tous les ${sort.cooldown} tours`);

  // La description du jeu d'abord : ce que fait le sort, avant ses chiffres.
  const description = texteDuJeu(sort.description);

  return `<h4>${echapper(sort.name)}</h4>`
    + `<div class="ligne1">${echapper(entete)}</div>`
    + (contraintes.length
      ? `<div class="contraintes">${echapper(majuscule(contraintes.join(' · ')))}</div>` : '')
    // La valeur comptée en gras, l'autre à côté.
    + (critEffets.length
      ? `<p class="critique">${critEffets.map((e) => `<strong>${echapper(majuscule(e.texte))}</strong>`
        + (e.applique ? ` en critique · ${e.autre} sans critique` : ` · ${e.autre} en critique`),
      ).join('<br>')}</p>`
      : '')
    + (description
      ? `<p class="description">${echapper(description).replace(/\n+/g, '<br>')}</p>` : '')
    + '<dl>'
    + `<dt>Coup critique</dt><dd>${echapper(majuscule(crit.texte))}`
    + (crit.calcul ? `<span class="calcul"> · ${echapper(crit.calcul)}</span>` : '') + '</dd>'
    + (lignes
      ? `<dt class="pleine">${table ? 'Dégâts sur votre build' : 'Jet de base'}</dt>${lignes}`
      : '<dt class="pleine">Ne frappe pas au lancer</dt>')
    + (limites.length ? `<dt>Limites</dt><dd>${echapper(majuscule(limites.join(' · ')))}</dd>` : '')
    + ((sort.sources || []).length
      ? `<dt>Équipement</dt><dd>${sort.sources.map(echapper).join('<br>')}</dd>` : '')
    + '</dl>'
    // Les hypothèses de modélisation ne s'affichent pas ; un point non tranché se
    // signale, ses chiffres étant une hypothèse et non une mesure.
    + ((sort.open_questions || []).length
      ? '<p class="question">Non vérifié en jeu.</p>'
      : '');
}

/// Le texte du jeu sans son balisage : un lien vers un objet garde son nom
/// (« {{item,23408::Dorigami}} »), une image de caractéristique disparaît.
function texteDuJeu(texte) {
  return (texte || '')
    .replace(/\{\{[^}]*::([^}]*)\}\}/g, '$1')
    .replace(/<[^>]+>/g, '')
    .trim();
}

// ---------------------------------------------------------------------
// La fiche d'un objet
// ---------------------------------------------------------------------

/// La fiche d'un objet de l'équipement, au survol de sa tuile : ses
/// caractéristiques, son effet spécial et le sort qu'il ajoute avec ce que la
/// Rotation en fait, puis ce qu'il fait hors du combat.
export function ficheDObjet(objet) {
  const valeur = (x) => (x.min === x.max ? nb(x.min) : `${nb(x.min)} à ${nb(x.max)}`);
  const stats = (objet.stats || [])
    .map((x) => `<dt>${echapper(x.label)}</dt><dd>${valeur(x)}</dd>`).join('');
  const section = (titre, corps, statut = '') => `<div class="section"><h5>${echapper(titre)}${statut}</h5>${corps}</div>`;
  const lignes = (liste) => `<p>${liste.map(echapper).join('<br>')}</p>`;
  const effets = (objet.effets || []).map((e) => {
    const texte = texteDuJeu(e.texte);
    return section(
      e.genre === 'sort_ajoute' ? 'Sort ajouté à la barre' : 'Effet spécial',
      `<strong>${echapper(e.nom)}</strong>`
        + (texte ? `<p>${echapper(texte).replace(/\n+/g, '<br>')}</p>` : ''),
      `<span class="statut${e.compte ? ' compte' : ''}">${echapper(e.statut)}</span>`,
    );
  }).join('');
  return `<h4>${echapper(objet.name)}</h4>`
    + `<div class="ligne1">${echapper(objet.label)} · niveau ${objet.level || '?'}</div>`
    + (stats ? `<dl>${stats}</dl>` : '')
    + ((objet.sorts || []).length ? section('Sorts de la classe', lignes(objet.sorts)) : '')
    + effets
    + ((objet.infos || []).length ? section('Aussi', lignes(objet.infos)) : '');
}
