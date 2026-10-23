// Le placement de la Rotation : le joueur pose son personnage et les ennemis
// sur un plateau des KrozTools (damier vide ou carte de boss), et chaque sort
// vise sa meilleure case depuis là, au lieu d'un nombre d'ennemis déclaré.
// Le placement et le plateau sont ceux des KrozTools (`etat.placement`,
// `etat.plateau`) : ce qu'on pose ici se retrouve dans KrozZone, et l'inverse.

import { contourZone, dessiner, losange, memeCase, versEcran } from './damier.js';
import { echapper, pluriel } from './format.js';
import { poutchs } from './pions.js';
import {
  bilanDuRecalage, chargerCartes, choisir, garderLeSol, plateauActif, pourLeServeur, recaler, selecteur,
} from './plateaux.js';

const placementDe = (etat) => etat.placement || (etat.placement = { lanceur: [-4, 0], visee: [0, 0], ennemis: [[0, 0]] });

export const actif = (etat) => Boolean(etat.rotation && etat.rotation.placement);

/// Le placement que la requête emporte, ou rien s'il n'est pas activé.
export function pourLaRequete(etat) {
  if (!actif(etat)) return null;
  const p = placementDe(etat);
  return {
    lanceur: p.lanceur,
    visee: p.ennemis[0] || p.lanceur,
    ennemis: p.ennemis,
    ...pourLeServeur(plateauActif(etat)),
  };
}

/// L'encart : une case pour placer le combat, puis le plateau et, à côté, ce
/// que chaque lancer du tour touche.
export function encart(etat) {
  const on = actif(etat);
  return `<div class="carte placement-rotation" id="cartePlacement">
    <label class="case-placement"><input type="checkbox" id="avecPlacement"${on ? ' checked' : ''}>
      <span>Placer le combat sur une carte</span></label>
    <div id="corpsPlacement"${on ? '' : ' hidden'}>
      <div class="barre-outils">
        <span class="outils-placement" role="radiogroup" aria-label="Ce que pose un clic">
          <label><input type="radio" name="outilPlacement" value="lanceur"> Vous</label>
          <label><input type="radio" name="outilPlacement" value="ennemi" checked> Ennemi</label>
        </span>
        <span id="choixPlateauPlacement">${selecteur('plateauPlacement', etat)}</span>
        <button type="button" class="action discret petit" id="viderPlacement">Retirer les ennemis</button>
      </div>
      <div class="placement-cote">
        <div>
          <div class="plateau" id="dessinPlacement"></div>
          <div class="legende-placement"><span><i class="vous"></i>vous</span><span><i class="ennemis"></i>ennemis</span>
            <span><i class="zone"></i>zone du sort</span><span><i class="obstacle"></i>obstacle</span></div>
        </div>
        <div class="lancers-placement" id="lancersPlacement"></div>
      </div>
      <p class="aide" id="etatPlacement"></p>
    </div>
  </div>`;
}

/// Branche l'encart. `change` prévient la Rotation que le placement a bougé :
/// le résultat affiché ne vaut plus pour lui.
export async function brancher(racine, etat, { appeler, change, maxEnnemis }) {
  const $ = (id) => racine.querySelector(`#${id}`);
  const placement = placementDe(etat);
  let refus = '';
  let resultat = null;
  let survol = null;

  const plateau = () => plateauActif(etat);
  const recalerTout = () => {
    const p = plateau();
    const ici = recaler(placement.lanceur, p);
    const deplace = ici && !memeCase(ici, placement.lanceur) ? 1 : 0;
    if (ici) placement.lanceur = ici;
    return bilanDuRecalage(deplace, garderLeSol(placement.ennemis, p));
  };

  function peindre() {
    const p = plateau();
    const sorts = (resultat && resultat.placement && resultat.placement.sorts) || {};
    const vise = survol ? sorts[survol.sort] : null;
    const dansZone = new Set(((vise && vise.zone) || []).map((c) => `${c[0]},${c[1]}`));
    let calques = '';
    if (vise && vise.zone.length) {
      calques += vise.zone.map((c) => `<polygon class="zone-fond" fill="var(--accent)" points="${losange(c[0], c[1])}"/>`).join('')
        + `<path class="zone-contour" stroke="var(--accent)" d="${contourZone(vise.zone)}"/>`
        + `<polygon class="visee-trait" points="${losange(vise.visee[0], vise.visee[1])}"/>`;
    }
    const [lx, ly] = versEcran(placement.lanceur[0], placement.lanceur[1]);
    const jetons = `<text class="jeton lanceur" x="${lx}" y="${ly}">V</text>`
      + poutchs(placement.ennemis.map((e, i) => ({ case: e, rang: i + 1 })));
    dessiner($('dessinPlacement'), {
      plateau: p,
      case: (x, y) => {
        const classes = [];
        if (dansZone.has(`${x},${y}`)) classes.push('zone');
        if (memeCase(placement.lanceur, [x, y])) classes.push('lanceur');
        if (placement.ennemis.some((e) => memeCase(e, [x, y]))) classes.push('ennemi');
        return {
          classes,
          titre: memeCase(placement.lanceur, [x, y]) ? 'Vous'
            : placement.ennemis.some((e) => memeCase(e, [x, y])) ? 'Ennemi' : '',
        };
      },
      calques,
      jetons,
      surClic: cliquer,
    });
    $('etatPlacement').textContent = refus;
  }

  /// À côté du plateau : les lancers du tour survolé (le premier sinon), et qui
  /// chacun touche ; puis les ennemis qu'aucune zone n'atteint.
  function lister() {
    const hote = $('lancersPlacement');
    if (!resultat || !resultat.placement) {
      hote.innerHTML = '<p class="aide">Calculez la rotation : chaque lancer dira qui il touche.</p>';
      return;
    }
    const sorts = resultat.placement.sorts || {};
    const tour = survol ? survol.tour : 0;
    const lancers = ((resultat.turns || [])[tour] || {}).casts || [];
    const titre = survol
      ? `Tour ${tour + 1}, survol de ${echapper((lancers[survol.lancer] || {}).name || '')}`
      : `Tour ${tour + 1}`;
    const ligne = (c, k) => {
      const s = sorts[c.id];
      const touche = s && s.touches.length
        ? `touche ${s.touches.map((i) => i + 1).join(', ')}` : 'ne touche personne';
      const icone = c.icone_objet ? `objets/${c.icone_objet}.webp` : c.dofusdb_id ? `icons/${c.dofusdb_id}.png` : null;
      return `<div class="lancer-place${survol && survol.lancer === k ? ' survole' : ''}">${
        icone ? `<img class="ic" src="${icone}" alt="">` : ''}${echapper(c.name)}<span class="touche">${touche}</span></div>`;
    };
    const loin = resultat.placement.hors_de_portee || [];
    hote.innerHTML = `<b>${titre}</b>${lancers.map(ligne).join('')}${loin.length ? `<p class="aide">${
      loin.length > 1 ? `Les ennemis ${loin.map((i) => i + 1).join(', ')} restent` : `L'ennemi ${loin[0] + 1} reste`}
      hors de portée de toutes les zones.</p>` : ''}`;
  }

  const outil = () => (racine.querySelector('input[name=outilPlacement]:checked') || {}).value || 'ennemi';

  function cliquer(x, y) {
    if (outil() === 'lanceur') {
      if (placement.ennemis.some((e) => memeCase(e, [x, y]))) return;
      placement.lanceur = [x, y];
    } else {
      const i = placement.ennemis.findIndex((e) => memeCase(e, [x, y]));
      if (i >= 0) placement.ennemis.splice(i, 1);
      else if (memeCase(placement.lanceur, [x, y])) return;
      else if (placement.ennemis.length >= maxEnnemis) {
        refus = `${maxEnnemis} ${pluriel(maxEnnemis, 'ennemi')} au plus : retirez-en un, en cliquant dessus, avant d'en poser un autre.`;
        peindre();
        return;
      } else placement.ennemis.push([x, y]);
    }
    refus = '';
    resultat = null;
    survol = null;
    peindre();
    lister();
    change();
  }

  $('avecPlacement').addEventListener('change', () => {
    etat.rotation.placement = $('avecPlacement').checked;
    $('corpsPlacement').hidden = !etat.rotation.placement;
    change();
  });
  $('viderPlacement').addEventListener('click', () => {
    placement.ennemis.splice(0);
    resultat = null;
    survol = null;
    peindre();
    lister();
    change();
  });
  racine.addEventListener('change', (e) => {
    if (e.target.id !== 'plateauPlacement') return;
    choisir(etat, e.target.value);
    refus = recalerTout();
    resultat = null;
    survol = null;
    peindre();
    lister();
    change();
  });

  // Les cartes de boss arrivent après le premier dessin : le sélecteur les
  // propose dès qu'elles sont là.
  await chargerCartes(etat, appeler);
  $('choixPlateauPlacement').innerHTML = selecteur('plateauPlacement', etat);
  refus = recalerTout();
  peindre();
  lister();

  return {
    /// Le résultat d'un calcul fait avec ce placement.
    montrer(r) {
      resultat = r && r.placement ? r : null;
      survol = null;
      peindre();
      lister();
    },
    /// Un lancer survolé dans la rotation : sa zone sur le plateau, son tour à
    /// côté ; `null` pour revenir au premier tour.
    survoler(s) {
      survol = s;
      peindre();
      lister();
    },
  };
}
