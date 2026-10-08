// Réseau de pièges : concevoir un réseau entier sur un plateau, ou le poser à la
// main et voir ce qui part quand on y entre. La géométrie et les règles restent
// au serveur (`crates/app/src/reseau.rs`) ; la page pose des coordonnées et
// dessine la réponse. Deux modes, deux pages : seuls le plateau, ses obstacles,
// les autres entités, les PA et le Chakra sont communs.

import {
  contourZone, dessiner, distance, losange, memeCase, rafraichirApercu, versEcran,
} from '../commun/damier.js';
import { COULEUR_ELEMENT, echapper, nb, pluriel } from '../commun/format.js';
import {
  fantome, icone, interdit, jetonLettre, poutchs, retrait,
} from '../commun/pions.js';
import {
  bilanDuRecalage, chargerCartes, choisir, garderLeSol, plateauActif, pourLeServeur,
  recaler, selecteur,
} from '../commun/plateaux.js';

/// Les deux modes de KrozTrap.
const MODES = [
  { id: 'concevoir', nom: 'Concevoir un réseau' },
  { id: 'main', nom: 'Poser à la main' },
];

/// Les tours de pose qu'un plan peut demander, comme le serveur.
const TOURS_MAX = 10;

export async function rendre(cible, etat, { appeler }) {
  // Ni Sram ni ennemi par défaut : le joueur se place et amène la cible lui-même.
  const reseau = etat.reseau || (etat.reseau = {
    lanceur: null,
    poses: [],
    entree: null,
    obstacles: [],
    choisi: null,
    chakra: false,
  });
  // Un état ouvert avant l'arrivée des alliés et des autres ennemis n'a pas
  // ces listes.
  reseau.allies = reseau.allies || [];
  reseau.ennemis = reseau.ennemis || [];
  reseau.mode = reseau.mode === 'main' ? 'main' : 'concevoir';
  reseau.tour = reseau.tour || 1;
  reseau.apercu = null;
  // Le concepteur : sa consigne, ses tours, et le plan qu'il a rendu, tant
  // que rien ne change sur le plateau.
  reseau.objectif = reseau.objectif === 'un_tour' ? 'un_tour' : 'frappe';
  reseau.toursPlan = Math.min(TOURS_MAX, Math.max(1, reseau.toursPlan || 1));
  reseau.plan = reseau.plan || null;
  reseau.pas = reseau.pas || 0;
  reseau.entreeChoisie = reseau.entreeChoisie || 0;
  // Les PA du tour partent de ceux du build de Sram chargé, douze sans lui.
  const deSram = Boolean(etat.build) && etat.resolu && etat.resolu.class === 4;
  reseau.pa = reseau.pa || (deSram && etat.resolu.ap) || 12;
  // Le damier ou une carte de boss, comme dans les autres outils.
  await chargerCartes(etat, appeler);

  cible.innerHTML = `
    <section class="outil">
      <div class="barre-outils" role="radiogroup" aria-label="Mode">
        ${MODES.map((m) => `<label class="segment"><input type="radio" name="modeReseau"
          value="${m.id}" ${reseau.mode === m.id ? 'checked' : ''}> ${m.nom}</label>`).join('')}
      </div>
      <div id="corpsReseau"></div>
    </section>`;
  const corps = cible.querySelector('#corpsReseau');
  const afficher = () => (reseau.mode === 'main' ? poserALaMain : concevoirUnReseau)(corps, etat, appeler);
  cible.querySelectorAll('input[name=modeReseau]').forEach((radio) => {
    radio.addEventListener('change', () => {
      reseau.mode = radio.value;
      afficher();
    });
  });
  await afficher();
}

// ---------------------------------------------------------------------------
// Ce que les deux modes dessinent de la même façon.
// ---------------------------------------------------------------------------

/// Ce que la page envoie pour un Sram : son équipement, ou un Sram de niveau 200
/// sans caractéristique, ce que la ligne d'état dit ; la géométrie ne dépend de
/// rien.
function leSram(etat) {
  const sram = Boolean(etat.build) && etat.resolu && etat.resolu.class === 4;
  return sram ? etat.build : { class: 4, level: 200 };
}

const estUnSram = (etat) => Boolean(etat.build && etat.resolu && etat.resolu.class === 4);

/// Un jeton de texte au centre d'une case.
function jeton(c, classe, texte) {
  const [jx, jy] = versEcran(c[0], c[1]);
  return `<text class="jeton ${classe}" x="${jx}" y="${jy}">${texte}</text>`;
}

/// Le dessin d'un piège : sa zone de déclenchement, son contour, son icône,
/// et sa pastille, qui porte `rang` s'il est donné.
function groupePiege(p, i, classe, rang) {
  if (!p.declenchement || !p.declenchement.length) return '';
  const couleur = COULEUR_ELEMENT[(p.elements || [])[0]] || 'var(--doux)';
  const [cx, cy] = versEcran(p.case[0], p.case[1]);
  return `<g class="piege-groupe${classe}" data-piege="${i}">`
    + p.declenchement.map((c) =>
      `<polygon class="zone-fond" fill="${couleur}" points="${losange(c[0], c[1])}"/>`).join('')
    + `<path class="zone-contour" stroke="${couleur}" d="${contourZone(p.declenchement)}"/>`
    + (p.dofusdb_id
      ? `<image class="piege-icone" href="icons/${p.dofusdb_id}.png" x="${cx - 14}" y="${cy - 30}" width="28" height="28"/>`
      : '')
    + `<circle class="piege-pastille" cx="${cx}" cy="${cy}" r="7"/>`
    + (rang != null ? `<text class="piege-rang" x="${cx}" y="${cy}">${rang}</text>` : '')
    + '</g>';
}

/// Le survol d'une case met en avant les pièges dont la zone la couvre.
/// Seules les classes changent : on évite de redessiner tout le damier à
/// chaque mouvement de souris.
function surSurvolDesPieges(hote, liste) {
  let survole = -1;
  return (x, y) => {
    const i = x == null ? -1
      : liste().findIndex((p) => (p.declenchement || []).some((c) => c[0] === x && c[1] === y));
    if (i === survole) return;
    survole = i;
    hote.querySelectorAll('.piege-groupe').forEach((g) => {
      g.classList.toggle('survole', Number(g.dataset.piege) === i);
    });
  };
}

/// Le trajet de chaque entité de la chaîne `ch`, jusqu'à l'étape `etape` (−1
/// : la chaîne entière), et la case où la cible en est. `depart` : la case
/// d'où la cible part.
function trajetDe(ch, depart, etape) {
  if (!ch || !depart || !ch.entites) return { chemin: '', arrivee: depart };
  const positions = ch.entites.map((e) => e.depart);
  const points = ch.entites.map((e) => [versEcran(e.depart[0], e.depart[1])]);
  ch.etapes.forEach((e, i) => {
    if (etape >= 0 && i > etape) return;
    // L'échange de Méprise : la cible prend la case du Sram, qui prend la
    // sienne. Le Sram est la deuxième entité.
    if (e.quoi === 'echange') {
      positions[0] = e.cible;
      points[0].push(versEcran(e.cible[0], e.cible[1]));
      if (points[1]) {
        positions[1] = e.sram;
        points[1].push(versEcran(e.sram[0], e.sram[1]));
      }
      return;
    }
    if (e.quoi !== 'deplace' || !e.fait) return;
    positions[e.entite] = e.vers;
    points[e.entite].push(versEcran(e.vers[0], e.vers[1]));
  });
  const chemin = points.map((p, i) => {
    if (p.length < 2) return '';
    const e = ch.entites[i];
    const camp = e.cible ? 'ennemi' : e.camp;
    return `<path class="trajet ${camp}" d="M ${p.map((q) => q.join(',')).join(' L ')}"/>`
      + p.slice(1).map((q) => `<circle class="trajet-pas ${camp}" cx="${q[0]}" cy="${q[1]}" r="3"/>`).join('');
  }).join('');
  return { chemin, arrivee: positions[0] };
}

/// Comment la page nomme une entité de la chaîne, par son numéro.
function quiDans(ch, i) {
  const e = ch && ch.entites && ch.entites[i];
  if (!e || e.cible) return { sujet: 'La cible', objet: 'la cible', feminin: true, camp: 'ennemi' };
  if (e.camp === 'sram') return { sujet: 'Le Sram', objet: 'le Sram', feminin: false, camp: 'sram' };
  if (e.camp === 'allie') {
    return { sujet: `Allié ${e.numero}`, objet: `l'allié ${e.numero}`, feminin: false, camp: 'allie' };
  }
  return { sujet: `Ennemi ${e.numero}`, objet: `l'ennemi ${e.numero}`, feminin: false, camp: 'ennemi' };
}

/// Les lignes de l'empilement des actions, une par étape de la chaîne ; celles
/// qui suivent l'étape `etape` pâlissent (−1 : aucune). Un piège qui figure dans
/// l'empilement s'est déclenché ; un piège qui ne part pas n'y figure pas.
function lignesDeLaPile(ch, catalogue, declencheur, etape) {
  const icone = (nom) => {
    const c = catalogue.find((x) => x.nom === nom);
    return c && c.dofusdb_id
      ? `<img src="icons/${c.dofusdb_id}.png" alt="" decoding="async" loading="lazy">` : '';
  };
  const iconeDuSort = () => (declencheur && declencheur.dofusdb_id
    ? `<img src="icons/${declencheur.dofusdb_id}.png" alt="" decoding="async" loading="lazy">` : '');
  const qui = (i) => quiDans(ch, i);
  return ch.etapes.map((e, i) => {
    const classes = [];
    if (etape >= 0 && i === etape) classes.push('courante');
    if (etape >= 0 && i > etape) classes.push('pale');
    let t;
    if (e.quoi === 'declenche') {
      t = `<span class="avec-icone">${icone(e.nom)}<span class="quoi">${echapper(e.nom)}</span></span>`;
    } else if (e.quoi === 'degats') {
      t = `<span class="degats">− ${nb(e.degats[0])} à ${nb(e.degats[1])}</span>`
        + `<br><span class="detail">Dégâts sur ${qui(e.entite).objet}</span>`;
    } else if (e.quoi === 'coup') {
      t = `<span class="avec-icone">${iconeDuSort()}<span class="degats">− ${nb(e.degats[0])} à ${
        nb(e.degats[1])}</span></span><br><span class="detail">${echapper(e.nom)} sur la cible</span>`;
    } else if (e.quoi === 'echange') {
      t = `<span class="quoi">${echapper(e.nom)} : la cible et le Sram échangent leurs places</span>`;
    } else if (e.quoi === 'deplace') {
      const q = qui(e.entite);
      // Les participes s'accordent à l'entité : « la cible arrêtée »,
      // « l'ennemi 1 arrêté ».
      const e_ = q.feminin ? 'e' : '';
      const verbe = (e.sens === 'attire' ? 'attiré' : 'poussé') + e_;
      // Le déplacement du sort qui amène l'ennemi, sans piège derrière lui.
      const par = e.piege === null ? `${echapper(e.nom)} : ` : '';
      t = `<span class="quoi">${par}${q.sujet} ${verbe} de ${e.fait} ${pluriel(e.fait, 'case')}</span>`
        + (e.bloquee
          ? `<br><span class="alerte detail">Bloqué${e_}, ${e.voulu} `
            + `${pluriel(e.voulu, 'case')} ${pluriel(e.voulu, 'voulue')}</span>`
          : e.arretee_par_piege
            ? `<br><span class="detail">Arrêté${e_} par un piège sur ${e.voulu}</span>`
            : '');
    } else if (e.quoi === 'poussee') {
      t = `<span class="degats">− ${nb(e.degats[0])}</span>`
        + `<br><span class="detail">Dommages de poussée sur ${qui(e.entite).objet}${
          e.percutee ? ', percuté' + (qui(e.entite).feminin ? 'e' : '') : ''}, ${e.cases} ${
          pluriel(e.cases, 'case')} ${pluriel(e.cases, 'restante')}</span>`;
    } else if (e.quoi === 'poison') {
      t = `<span class="detail">Poison sur ${qui(e.entite).objet}, − ${nb(e.degats[0])} à
        ${nb(e.degats[1])} en fin de tour, hors du total</span>`;
    } else if (e.quoi === 'chakra') {
      t = `<span class="degats">− ${nb(e.degats[0])} à ${nb(e.degats[1])}</span>`
        + '<br><span class="detail">Concentration de Chakra, vol de vie</span>';
    } else if (e.quoi === 'fin') {
      // Le Sram est toujours là : au-delà de deux entités, d'autres attendent.
      t = ch.entites && ch.entites.length > 2
        ? '<span class="quoi">Plus rien ne bouge</span>'
        : '<span class="quoi">La cible s\'arrête</span>';
    } else {
      t = '<span class="alerte">trace coupée</span>';
    }
    return `<li class="${classes.join(' ')}">${t}</li>`;
  }).join('');
}

/// Ce que résume la ligne sous l'empilement.
function resumeDeLaPile(ch, etape) {
  return `${ch.etapes.length} ${pluriel(ch.etapes.length, 'étape')}`
    + (etape >= 0 ? ` · arrêté à la ${etape + 1}` : '')
    + ` · ordre ${ch.ordre === 'pose' ? 'de pose (FIFO)' : ch.ordre}`;
}

/// Ce que le changement de plateau retire ou ramène : les pièces hors du sol
/// partent, le Sram revient sur la case libre la plus proche.
function recalerReseau(reseau, plateau) {
  let retires = garderLeSol(reseau.poses, plateau, (q) => q.case)
    + garderLeSol(reseau.obstacles, plateau)
    + garderLeSol(reseau.allies, plateau)
    + garderLeSol(reseau.ennemis, plateau);
  if (reseau.entree && !plateau.estSol(reseau.entree)) { reseau.entree = null; retires += 1; }
  const ici = reseau.lanceur && recaler(reseau.lanceur, plateau, [reseau.entree].filter(Boolean));
  const deplace = ici && !memeCase(ici, reseau.lanceur) ? 1 : 0;
  if (ici) reseau.lanceur = ici;
  return bilanDuRecalage(deplace, retires);
}

// ---------------------------------------------------------------------------
// Concevoir un réseau : le plan entier, lu tour par tour.
// ---------------------------------------------------------------------------

/// Le concepteur : sur un plateau, avec ses murs et ses trous, le meilleur réseau
/// compact pour le nombre de tours donné, et ses entrées possibles à la fin. Le
/// joueur lit le plan tour par tour, dans les deux sens, puis le déclenchement.
async function concevoirUnReseau(hote, etat, appeler) {
  const reseau = etat.reseau;
  let plateau = plateauActif(etat);
  let bilan = recalerReseau(reseau, plateau);
  // La chaîne du plan depuis l'entrée choisie : elle porte aussi la zone et
  // l'élément de chaque pose, que le plateau dessine à chaque tour.
  let reponse = null;
  let etape = -1;

  hote.innerHTML = `
    <p class="sous">Choisissez un plateau et une consigne, puis concevez le réseau.</p>
    <div class="barre-outils">${selecteur('choixPlateauReseau', etat)}</div>
    <div class="barre-outils">
      <label class="segment"><input type="radio" name="objectifReseau" value="frappe"
        ${reseau.objectif === 'frappe' ? 'checked' : ''}> Sur plusieurs tours (OS)</label>
      <label class="segment"><input type="radio" name="objectifReseau" value="un_tour"
        ${reseau.objectif === 'un_tour' ? 'checked' : ''}> En un tour</label>
      <label class="champ-pa" for="toursReseau" id="champTours">Tours de pose
        <select id="toursReseau">${Array.from({ length: TOURS_MAX }, (_, i) => i + 1).map((n) =>
          `<option value="${n}" ${n === reseau.toursPlan ? 'selected' : ''}>${n}</option>`).join('')}</select></label>
      <label class="champ-pa" for="paTour">PA par tour
        <input type="number" id="paTour" min="1" max="30" step="1" value="${reseau.pa}"></label>
      <button type="button" class="action" id="concevoirReseau">Concevoir le réseau</button>
    </div>
    <div class="barre-outils">
      <label class="segment"><input type="radio" name="outilReseau" value="mur" checked> Obstacle</label>
      <label class="segment"><input type="radio" name="outilReseau" value="allie"> Un allié</label>
      <label class="segment"><input type="radio" name="outilReseau" value="ennemi"> Un autre ennemi</label>
      <button type="button" class="action discret" id="viderReseau">Tout retirer</button>
    </div>
    <label class="bascule" for="chakraReseau">
      <input type="checkbox" id="chakraReseau" ${reseau.chakra ? 'checked' : ''}>
      <span>Concentration de Chakra sur l'ennemi</span>
    </label>
    <div id="planReseau" class="plan"></div>
    <div class="vue-damier">
      <div>
        <div class="plateau" id="plateauReseau"></div>
        <p class="aide" id="etatReseau"></p>
      </div>
      <div id="panneauPlan" hidden></div>
    </div>`;

  const $ = (id) => hote.querySelector(`#${id}`);
  const outilActif = () => (hote.querySelector('input[name=outilReseau]:checked') || {}).value || 'mur';
  const plan = () => (reseau.plan && !reseau.plan.raison ? reseau.plan : null);
  /// Les étapes du plan : un tour de pose chacune, puis le déclenchement.
  const etapes = () => (plan() ? plan().tours + 1 : 0);
  const auDeclenchement = () => plan() && reseau.pas >= plan().tours;
  const entreeChoisie = () => {
    const p = plan();
    return p && (p.entrees[reseau.entreeChoisie] || p.entrees[0] || { case: p.entree, total: p.total });
  };
  /// Les poses du plan telles que le serveur les rend : zone, élément, icône.
  const dessinees = () => (reponse && reponse.poses) || [];

  /// Le plateau change : le plan ne vaut plus.
  function oublierLePlan() {
    reseau.plan = null;
    reseau.pas = 0;
    reseau.entreeChoisie = 0;
    reponse = null;
    etape = -1;
  }

  /// Ce qu'un clic poserait sous le curseur : un obstacle, un allié, un autre
  /// ennemi, ou une croix s'il retirerait ce qui est là.
  function apercu(x, y) {
    const ici = [x, y];
    const quoi = outilActif();
    const surMur = reseau.obstacles.some((c) => memeCase(c, ici));
    const surAllie = reseau.allies.some((c) => memeCase(c, ici));
    const surEnnemi = reseau.ennemis.some((c) => memeCase(c, ici));
    if (quoi === 'mur') {
      return surMur ? retrait(ici) : fantome(`<polygon class="case mur" points="${losange(x, y)}"/>`);
    }
    if (quoi === 'allie') {
      return surAllie ? retrait(ici) : fantome(jetonLettre(ici, 'allie', `A${reseau.allies.length + 1}`));
    }
    return surEnnemi ? retrait(ici)
      : fantome(poutchs([{ case: ici, rang: `E${reseau.ennemis.length + 1}` }]));
  }

  /// Retire l'allié ou l'autre ennemi posé sur cette case, s'il y en a un.
  function libererCase(ici) {
    [reseau.allies, reseau.ennemis].forEach((liste) => {
      const i = liste.findIndex((c) => memeCase(c, ici));
      if (i >= 0) liste.splice(i, 1);
    });
  }

  function cliquer(x, y) {
    const quoi = outilActif();
    const ici = [x, y];
    oublierLePlan();
    bilan = '';
    // ⚠️ UNE CASE NE TIENT QU'UNE ENTITÉ : un mur chasse l'allié ou l'ennemi
    // qui s'y tenait.
    if (quoi === 'mur') {
      const i = reseau.obstacles.findIndex((c) => memeCase(c, ici));
      if (i >= 0) reseau.obstacles.splice(i, 1);
      else {
        reseau.obstacles.push(ici);
        libererCase(ici);
      }
    } else {
      const liste = quoi === 'allie' ? reseau.allies : reseau.ennemis;
      const deja = liste.some((c) => memeCase(c, ici));
      libererCase(ici);
      const mur = reseau.obstacles.findIndex((c) => memeCase(c, ici));
      if (mur >= 0) reseau.obstacles.splice(mur, 1);
      if (!deja) liste.push(ici);
    }
    peindre();
  }

  $('choixPlateauReseau').addEventListener('change', () => {
    choisir(etat, $('choixPlateauReseau').value);
    plateau = plateauActif(etat);
    bilan = recalerReseau(reseau, plateau);
    oublierLePlan();
    peindre();
  });
  hote.querySelectorAll('input[name=objectifReseau]').forEach((radio) => {
    radio.addEventListener('change', () => {
      reseau.objectif = radio.value;
      oublierLePlan();
      peindre();
    });
  });
  $('toursReseau').addEventListener('change', () => {
    reseau.toursPlan = Number($('toursReseau').value) || 1;
    oublierLePlan();
    peindre();
  });
  $('paTour').addEventListener('change', () => {
    const v = Math.round(Number($('paTour').value));
    if (Number.isFinite(v) && v >= 1) reseau.pa = Math.min(30, v);
    $('paTour').value = reseau.pa;
    oublierLePlan();
    peindre();
  });
  $('chakraReseau').addEventListener('change', () => {
    reseau.chakra = $('chakraReseau').checked;
    oublierLePlan();
    peindre();
  });
  hote.querySelectorAll('input[name=outilReseau]').forEach((radio) => {
    radio.addEventListener('change', () => rafraichirApercu($('plateauReseau')));
  });
  $('viderReseau').addEventListener('click', () => {
    reseau.obstacles = [];
    reseau.allies = [];
    reseau.ennemis = [];
    oublierLePlan();
    peindre();
  });
  $('concevoirReseau').addEventListener('click', concevoir);

  /// Ce que la page envoie au serveur : le plateau et ce qui s'y tient.
  function base() {
    return {
      ...leSram(etat),
      ...pourLeServeur(plateau),
      lanceur: null,
      obstacles: reseau.obstacles,
      allies: reseau.allies,
      ennemis: reseau.ennemis,
      chakra: reseau.chakra,
      pa: reseau.pa,
    };
  }

  async function concevoir() {
    const bouton = $('concevoirReseau');
    bouton.disabled = true;
    bouton.textContent = 'Recherche…';
    oublierLePlan();
    try {
      const r = await appeler('reseau', {
        ...base(),
        poses: [],
        concevoir: { objectif: reseau.objectif, tours: reseau.toursPlan },
      });
      reseau.plan = r.plan && !r.plan.raison ? r.plan
        : { raison: (r.plan && r.plan.raison) || 'Aucun réseau trouvé.' };
    } catch (e) {
      reseau.plan = { raison: e.message };
    }
    bouton.disabled = false;
    bouton.textContent = 'Concevoir le réseau';
    await chaine();
  }

  /// La chaîne du plan depuis l'entrée choisie, et le dessin de ses poses.
  async function chaine() {
    const p = plan();
    reponse = null;
    etape = -1;
    if (p) {
      try {
        reponse = await appeler('reseau', {
          ...base(),
          poses: p.poses.map((q) => ({ sort: q.sort, case: q.case, tour: q.tour })),
          entree: entreeChoisie().case,
        });
      } catch (e) {
        reseau.plan = { raison: e.message };
      }
    }
    peindre();
  }

  /// Aller à une étape du plan : un tour de pose, ou le déclenchement.
  function allerA(pas) {
    if (!plan()) return;
    reseau.pas = Math.max(0, Math.min(etapes() - 1, pas));
    etape = -1;
    peindre();
    const bouton = hote.querySelector(`#planReseau button[data-pas="${reseau.pas}"]`);
    if (bouton) bouton.focus();
  }

  /// Les tours de pose ne valent que pour la consigne sur plusieurs tours ; le nom
  /// de chaque consigne suffit.
  function dessinerConsigne() {
    $('champTours').hidden = reseau.objectif === 'un_tour';
  }

  /// Le résumé du plan et ses étapes, au-dessus du plateau.
  function dessinerPlan() {
    const bloc = $('planReseau');
    const r = reseau.plan;
    if (!r) { bloc.innerHTML = ''; return; }
    if (r.raison) {
      bloc.innerHTML = `<p class="bandeau souci">${echapper(r.raison)}</p>`;
      return;
    }
    const n = r.poses.length;
    const [larg, haut] = r.emprise;
    const avecSort = r.declencheur ? ` et ${echapper(r.declencheur.nom)}` : '';
    const resume = `<strong>${nb(r.total[0])} à ${nb(r.total[1])}</strong> dégâts · ${n} ${
      pluriel(n, 'piège')}${avecSort}, ${r.pa} PA ${r.tours > 1 ? `en ${r.tours} tours` : 'en un tour'}`
      + (larg * haut > 1 ? ` · ${larg} × ${haut} cases` : ' · une case');
    const noms = [...Array.from({ length: r.tours }, (_, i) => `Tour ${i + 1}`), 'Déclenchement'];
    bloc.innerHTML = `<p class="bandeau bon">${resume}</p>
      <div class="barre-outils plan-etapes" role="group" aria-label="Étapes du plan">
        <button type="button" class="action discret" data-aller="-1" aria-label="Étape précédente"
          ${reseau.pas <= 0 ? 'disabled' : ''}>◀</button>
        ${noms.map((nom, i) => `<button type="button" class="etape" data-pas="${i}"
          aria-pressed="${i === reseau.pas}">${nom}</button>`).join('')}
        <button type="button" class="action discret" data-aller="1" aria-label="Étape suivante"
          ${reseau.pas >= noms.length - 1 ? 'disabled' : ''}>▶</button>
      </div>`;
    bloc.querySelectorAll('button[data-pas]').forEach((b) => {
      b.addEventListener('click', () => allerA(Number(b.dataset.pas)));
    });
    bloc.querySelectorAll('button[data-aller]').forEach((b) => {
      b.addEventListener('click', () => allerA(reseau.pas + Number(b.dataset.aller)));
    });
    // Les flèches du clavier passent d'une étape à l'autre.
    bloc.querySelector('.plan-etapes').addEventListener('keydown', (ev) => {
      if (ev.key === 'ArrowLeft') { ev.preventDefault(); allerA(reseau.pas - 1); }
      if (ev.key === 'ArrowRight') { ev.preventDefault(); allerA(reseau.pas + 1); }
    });
  }

  /// Le panneau à droite du plateau : ce qu'il faut poser au tour affiché,
  /// dans l'ordre ; au déclenchement, les entrées et l'empilement.
  function dessinerPanneau() {
    const bloc = $('panneauPlan');
    const p = plan();
    bloc.hidden = !p;
    if (!p) { bloc.innerHTML = ''; return; }
    if (!auDeclenchement()) {
      const tour = reseau.pas + 1;
      const duTour = p.poses.filter((q) => q.tour === tour);
      bloc.innerHTML = `<div class="pile poses">
          <h4>Tour ${tour} sur ${p.tours} · ${p.pa_par_tour[tour - 1]} PA sur ${p.pa_du_tour}</h4>
          <ol>${duTour.map((q) => `<li><span class="avec-icone">${q.dofusdb_id
            ? `<img src="icons/${q.dofusdb_id}.png" alt="" decoding="async" loading="lazy">` : ''}
            <span class="quoi">${echapper(q.nom)}</span><span class="detail">${q.pa} PA</span></span></li>`).join('')}
          ${p.declencheur ? `<li><span class="avec-icone">${p.declencheur.dofusdb_id
            ? `<img src="icons/${p.declencheur.dofusdb_id}.png" alt="" decoding="async" loading="lazy">` : ''}
            <span class="quoi">${[p.declencheur, ...(p.aussi || [])].map((t) => echapper(t.nom)).join(' ou ')}</span>
            <span class="detail">${p.declencheur.pa} PA, sur l'ennemi</span></span></li>` : ''}</ol>
        </div>
        <p class="aide">Posez-les dans l'ordre des numéros.</p>`;
      return;
    }
    const choisie = entreeChoisie();
    const rang = p.entrees.indexOf(choisie);
    const ch = reponse && reponse.chaine;
    const entrees = p.entrees.map((e, i) => `<li><button type="button" class="entree-choix"
        data-entree="${i}" aria-pressed="${i === rang}"><strong>E${i + 1}</strong>
        <span class="detail">${nb(e.total[0])} à ${nb(e.total[1])}</span></button></li>`).join('');
    // Le sort d'entrée, et ceux du même prix : le joueur lance celui qu'il a
    // équipé et qu'il peut placer. Le total compte celui qui frappe le plus.
    const sortDEntree = (t) => `<strong>${echapper(t.nom)}</strong> (${t.pa} PA${
      t.degats[1] > 0 ? `, ${nb(t.degats[0])} à ${nb(t.degats[1])} dégâts` : ', sans dégâts'})`;
    const tous = p.poses.length > 1 ? `les ${p.poses.length} pièges` : 'le piège';
    const consigne = p.declencheur
      ? `Faites entrer l'ennemi sur la case <strong>E${rang + 1}</strong> avec ${
        [p.declencheur, ...(p.aussi || [])].map(sortDEntree).join(' ou ')} : il prend ${tous}.`
      : `Amenez l'ennemi sur la case <strong>E${rang + 1}</strong> : il prend ${tous}.`;
    bloc.innerHTML = `<p class="bandeau bon">${consigne} <strong>${nb(choisie.total[0])} à ${
        nb(choisie.total[1])}</strong> dégâts${p.aussi && p.aussi.length
        ? ` avec ${echapper(p.declencheur.nom)}` : ''}.</p>
      <div class="pile entrees">
        <h4>${p.entrees.length > 1 ? `${p.entrees.length} cases d'entrée` : 'La case d\'entrée'}</h4>
        <ol>${entrees}</ol>
      </div>
      ${ch ? `<div class="pile" style="margin-top:.6rem">
        <h4>Empilement des actions</h4>
        <ol>${lignesDeLaPile(ch, (reponse && reponse.catalogue) || [], null, etape)}</ol>
      </div>
      <div class="barre-outils" style="margin-top:.5rem">
        <button type="button" class="action discret" data-pas-chaine="-1" aria-label="Étape d'avant">◀</button>
        <button type="button" class="action discret" data-pas-chaine="1" aria-label="Étape d'après">▶</button>
        <button type="button" class="action discret" data-pas-chaine="0">Tout</button>
      </div>
      <p class="aide">${resumeDeLaPile(ch, etape)}</p>` : ''}`;
    bloc.querySelectorAll('button[data-entree]').forEach((b) => {
      b.addEventListener('click', async () => {
        reseau.entreeChoisie = Number(b.dataset.entree);
        await chaine();
      });
    });
    bloc.querySelectorAll('button[data-pas-chaine]').forEach((b) => {
      b.addEventListener('click', () => {
        const n = (ch && ch.etapes.length) || 0;
        const d = Number(b.dataset.pasChaine);
        if (d === 0) etape = -1;
        else if (d < 0) etape = etape < 0 ? n - 1 : Math.max(0, etape - 1);
        else etape = etape < 0 ? 0 : Math.min(n - 1, etape + 1);
        peindre();
      });
    });
  }

  function peindre() {
    const p = plan();
    const final = auDeclenchement();
    const tour = p ? reseau.pas + 1 : 0;
    const liste = dessinees();
    const choisie = final ? entreeChoisie() : null;
    const t = final ? trajetDe(reponse && reponse.chaine, choisie && choisie.case, etape)
      : { chemin: '', arrivee: null };

    // Au tour affiché : ses poses numérotées dans leur ordre, celles des tours
    // d'avant pâles et sans numéro, celles d'après absentes. Au
    // déclenchement : tout le réseau, numéroté dans l'ordre de pose.
    let rangDuTour = 0;
    const groupes = !p ? '' : liste.map((q, i) => {
      const t_ = (p.poses[i] && p.poses[i].tour) || 1;
      if (final) return groupePiege(q, i, '', i + 1);
      if (t_ > tour) return '';
      if (t_ < tour) return groupePiege(q, i, ' anterieur', null);
      rangDuTour += 1;
      return groupePiege(q, i, ' du-tour', rangDuTour);
    }).join('');

    let jetons = '';
    reseau.allies.forEach((c, i) => { jetons += jeton(c, 'allie', `A${i + 1}`); });
    jetons += poutchs(reseau.ennemis.map((c, i) => ({ case: c, rang: `E${i + 1}` })));
    // Les entrées, au déclenchement seulement : celle qu'on suit en plein, les autres
    // plus claires. La marque se tient sur le bord gauche de la case, une entrée
    // portant souvent un piège au centre.
    if (final) {
      p.entrees.forEach((e, i) => {
        const [ex, ey] = versEcran(e.case[0], e.case[1]);
        const classe = e === choisie ? 'entree' : 'entree autre';
        jetons += `<polygon class="entree-contour${e === choisie ? '' : ' autre'}" points="${
          losange(e.case[0], e.case[1])}"/>`
          + `<rect class="entree-etiquette${e === choisie ? '' : ' autre'}" x="${ex - 22}" y="${ey - 7}"`
          + ' width="19" height="14" rx="7"/>'
          + `<text class="jeton ${classe}" x="${ex - 12.5}" y="${ey}">E${i + 1}</text>`;
      });
    }

    dessiner($('plateauReseau'), {
      plateau,
      apercu,
      case: (x, y) => {
        const classes = [];
        const ici = [x, y];
        const estMur = reseau.obstacles.some((c) => memeCase(c, ici));
        const entree = final ? p.entrees.findIndex((e) => memeCase(e.case, ici)) : -1;
        const estArrivee = final && t.arrivee && memeCase(t.arrivee, ici) && !memeCase(choisie.case, ici);
        if (estMur) classes.push('mur');
        if (entree >= 0) classes.push('entree');
        if (estArrivee) classes.push('arrivee-case');
        return {
          classes,
          titre: entree >= 0 ? `Entrée E${entree + 1} : ${nb(p.entrees[entree].total[0])} à ${
            nb(p.entrees[entree].total[1])} dégâts`
            : estArrivee ? 'Où la cible finit'
            : estMur ? 'Obstacle'
            : 'Case libre',
        };
      },
      calques: groupes + t.chemin,
      jetons,
      surClic: cliquer,
      surSurvol: surSurvolDesPieges($('plateauReseau'), dessinees),
    });

    $('etatReseau').textContent = (estUnSram(etat) ? '' : 'Dégâts d\'un Sram niveau 200 sans équipement.')
      + (bilan || '');
    dessinerConsigne();
    dessinerPlan();
    dessinerPanneau();
  }

  if (plan() && !reponse) await chaine();
  else peindre();
}

// ---------------------------------------------------------------------------
// Poser à la main : le simulateur, tour par tour.
// ---------------------------------------------------------------------------

async function poserALaMain(hote, etat, appeler) {
  const reseau = etat.reseau;
  let reponse = null;
  // −1 affiche la chaîne entière ; sinon on s'arrête à cette étape.
  let etape = -1;
  // Le serveur fait des murs et des trous du plateau des obstacles qui arrêtent
  // les poussées, et ne pose de piège que sur son sol.
  let plateau = plateauActif(etat);
  let bilan = recalerReseau(reseau, plateau);

  hote.innerHTML = `
    <p class="sous">Posez vos pièges et la case d'entrée de l'ennemi.</p>
    <div id="palettePieges" class="palette"></div>
    <div class="barre-outils" style="margin-top:.7rem">${selecteur('choixPlateauReseau', etat)}</div>
    <div class="barre-outils">
      <label class="segment"><input type="radio" name="outilReseau" value="piege" checked> Poser un piège</label>
      <label class="segment"><input type="radio" name="outilReseau" value="entree"> Case d'entrée</label>
      <label class="segment"><input type="radio" name="outilReseau" value="mur"> Obstacle</label>
      <label class="segment"><input type="radio" name="outilReseau" value="sram"> Le Sram</label>
      <label class="segment"><input type="radio" name="outilReseau" value="allie"> Un allié</label>
      <label class="segment"><input type="radio" name="outilReseau" value="ennemi"> Un autre ennemi</label>
      <button type="button" class="action discret" id="viderReseau">Tout retirer</button>
    </div>
    <label class="bascule" for="chakraReseau">
      <input type="checkbox" id="chakraReseau" ${reseau.chakra ? 'checked' : ''}>
      <span>Concentration de Chakra sur l'ennemi</span>
    </label>
    <div class="barre-outils" style="margin-top:.7rem">
      <button type="button" class="action discret" id="tourPrecedent">◀ Tour précédent</button>
      <span class="tour-courant" id="tourCourant"></span>
      <button type="button" class="action discret" id="tourSuivant">Tour suivant ▶</button>
      <label class="champ-pa" for="paTour">PA par tour
        <input type="number" id="paTour" min="1" max="30" step="1" value="${reseau.pa || ''}"></label>
      <button type="button" class="action discret" id="proposerTour">Proposer le tour</button>
    </div>
    <p id="propositionReseau" class="bandeau" hidden></p>

    <div class="vue-damier">
      <div>
        <div class="plateau" id="plateauReseau"></div>
        <p class="aide" id="etatReseau"></p>
        <p id="arrivee" class="bandeau"></p>
      </div>
      <div id="pileBloc" hidden>
        <div class="pile">
          <h4>Empilement des actions</h4>
          <ol id="pileListe"></ol>
        </div>
        <div class="barre-outils" style="margin-top:.5rem">
          <button type="button" class="action discret" id="pasPrecedent">◀</button>
          <button type="button" class="action discret" id="pasSuivant">▶</button>
          <button type="button" class="action discret" id="pasTout">Tout</button>
        </div>
        <p class="aide" id="pileResume"></p>
      </div>
    </div>`;

  const $ = (id) => hote.querySelector(`#${id}`);
  /// Ce qui est au sol à la fin du tour affiché, dans l'ordre de pose : les
  /// tours d'abord, puis l'ordre de pose de chacun.
  const auSol = () => reseau.poses
    .map((p, i) => ({ p, i }))
    .filter(({ p }) => (p.tour || 1) <= reseau.tour)
    .sort((a, b) => (a.p.tour || 1) - (b.p.tour || 1) || a.i - b.i)
    .map(({ p }) => p);
  let envoyees = 0;
  const catalogue = () => (reponse && reponse.catalogue) || [];
  const poses = () => (reponse && reponse.poses) || [];
  const chaine = () => (reponse && reponse.chaine) || null;

  const outilActif = () =>
    (hote.querySelector('input[name=outilReseau]:checked') || {}).value || 'piege';

  /// Ce qu'un clic poserait sous le curseur. Pour un piège : sa zone de
  /// déclenchement et son icône, cernées de rouge si le jeu la refuserait (case
  /// prise, hors de portée du Sram) ; une croix si le clic retirerait ce qui est
  /// là.
  function apercu(x, y) {
    const ici = [x, y];
    const quoi = outilActif();
    const surSram = memeCase(reseau.lanceur, ici);
    const surEntree = memeCase(reseau.entree, ici);
    const surMur = reseau.obstacles.some((c) => memeCase(c, ici));
    const surAllie = reseau.allies.some((c) => memeCase(c, ici));
    const surEnnemi = reseau.ennemis.some((c) => memeCase(c, ici));
    if (quoi === 'piege') {
      if (reseau.poses.some((p) => memeCase(p.case, ici))) return retrait(ici);
      const modele = catalogue().find((c) => c.sort === reseau.choisi);
      if (!modele) return '';
      const [min, max] = modele.portee || [0, Infinity];
      // Sans Sram sur le damier, la portée ne se juge pas.
      const d = reseau.lanceur ? distance(reseau.lanceur, ici) : min;
      const refusee = surSram || surMur || surAllie || surEnnemi || d < min || d > max;
      const cases = (modele.gabarit || [[0, 0]]).map(([dx, dy]) => [x + dx, y + dy]);
      const couleur = COULEUR_ELEMENT[(modele.elements || [])[0]] || 'var(--doux)';
      return (refusee ? interdit(ici) : '') + fantome('<g class="fantome">'
        + cases.map((c) => `<polygon class="zone-fond" fill="${couleur}" points="${losange(c[0], c[1])}"/>`).join('')
        + `<path class="zone-contour" stroke="${couleur}" d="${contourZone(cases)}"/></g>`
        + icone(ici, modele.dofusdb_id));
    }
    if (quoi === 'entree') {
      if (surSram) return interdit(ici);
      return surEntree ? retrait(ici) : fantome(jetonLettre(ici, 'entree', 'E'));
    }
    if (quoi === 'mur') {
      return surMur ? retrait(ici) : fantome(`<polygon class="case mur" points="${losange(x, y)}"/>`);
    }
    if (quoi === 'sram') {
      if (surEntree) return interdit(ici);
      return surSram ? retrait(ici) : fantome(jetonLettre(ici, 'lanceur', 'S'));
    }
    if (surSram || surEntree) return interdit(ici);
    if (quoi === 'allie') {
      return surAllie ? retrait(ici) : fantome(jetonLettre(ici, 'allie', `A${reseau.allies.length + 1}`));
    }
    return surEnnemi ? retrait(ici)
      : fantome(poutchs([{ case: ici, rang: `E${reseau.ennemis.length + 1}` }]));
  }

  /// Retire l'allié ou l'autre ennemi posé sur cette case, s'il y en a un.
  function libererCase(ici) {
    [reseau.allies, reseau.ennemis].forEach((liste) => {
      const i = liste.findIndex((c) => memeCase(c, ici));
      if (i >= 0) liste.splice(i, 1);
    });
  }

  function cliquer(x, y) {
    const quoi = outilActif();
    const ici = [x, y];
    const commun = () => JSON.stringify([reseau.obstacles, reseau.allies, reseau.ennemis]);
    const avant = commun();
    // Le damier change : ce que le générateur avait proposé ne vaut plus.
    reseau.apercu = null;
    bilan = '';
    // ⚠️ UNE CASE NE TIENT QU'UNE ENTITÉ. Le Sram, l'ennemi qui entre et un mur
    // chassent l'allié ou l'ennemi qui s'y tenait ; le Sram et l'entrée ne se
    // superposent pas.
    if (quoi === 'sram') {
      if (memeCase(reseau.entree, ici)) return;
      // Un clic sur le Sram le retire du damier.
      reseau.lanceur = memeCase(reseau.lanceur, ici) ? null : ici;
      libererCase(ici);
    } else if (quoi === 'entree') {
      if (memeCase(reseau.lanceur, ici)) return;
      reseau.entree = memeCase(reseau.entree, ici) ? null : ici;
      libererCase(ici);
    } else if (quoi === 'mur') {
      const i = reseau.obstacles.findIndex((c) => memeCase(c, ici));
      if (i >= 0) reseau.obstacles.splice(i, 1);
      else {
        reseau.obstacles.push(ici);
        libererCase(ici);
      }
    } else if (quoi === 'allie' || quoi === 'ennemi') {
      if (memeCase(reseau.lanceur, ici) || memeCase(reseau.entree, ici)) return;
      const liste = quoi === 'allie' ? reseau.allies : reseau.ennemis;
      const deja = liste.some((c) => memeCase(c, ici));
      libererCase(ici);
      const mur = reseau.obstacles.findIndex((c) => memeCase(c, ici));
      if (mur >= 0) reseau.obstacles.splice(mur, 1);
      if (!deja) liste.push(ici);
    } else {
      // Un piège d'un tour à venir, masqué ici, garde sa case.
      const i = reseau.poses.findIndex((p) => memeCase(p.case, [x, y]));
      if (i >= 0 && (reseau.poses[i].tour || 1) <= reseau.tour) reseau.poses.splice(i, 1);
      else if (i < 0 && reseau.choisi) reseau.poses.push({ sort: reseau.choisi, case: [x, y], tour: reseau.tour });
    }
    // Un obstacle, un allié ou un ennemi de plus ou de moins : le plateau du
    // concepteur change aussi, et son plan ne vaut plus.
    if (commun() !== avant) reseau.plan = null;
    etape = -1;
    interroger();
  }

  $('choixPlateauReseau').addEventListener('change', () => {
    choisir(etat, $('choixPlateauReseau').value);
    plateau = plateauActif(etat);
    reseau.apercu = null;
    reseau.plan = null;
    bilan = recalerReseau(reseau, plateau);
    interroger();
  });
  $('chakraReseau').addEventListener('change', () => {
    reseau.chakra = $('chakraReseau').checked;
    reseau.plan = null;
    interroger();
  });
  $('viderReseau').addEventListener('click', () => {
    reseau.poses = [];
    reseau.entree = null;
    reseau.obstacles = [];
    reseau.allies = [];
    reseau.ennemis = [];
    reseau.tour = 1;
    reseau.apercu = null;
    reseau.plan = null;
    etape = -1;
    interroger();
  });
  $('paTour').addEventListener('change', () => {
    const v = Math.round(Number($('paTour').value));
    if (Number.isFinite(v) && v >= 1) reseau.pa = Math.min(30, v);
    $('paTour').value = reseau.pa;
    reseau.apercu = null;
    reseau.plan = null;
    interroger();
  });
  hote.querySelectorAll('input[name=outilReseau]').forEach((radio) => {
    radio.addEventListener('change', () => rafraichirApercu($('plateauReseau')));
  });
  $('proposerTour').addEventListener('click', proposer);
  // Passer d'un tour à l'autre, dans les deux sens : le damier montre ce qui
  // est au sol à la fin du tour affiché.
  $('tourPrecedent').addEventListener('click', () => {
    if (reseau.tour <= 1) return;
    reseau.tour -= 1;
    reseau.apercu = null;
    etape = -1;
    interroger();
  });
  $('tourSuivant').addEventListener('click', () => {
    reseau.tour += 1;
    reseau.apercu = null;
    etape = -1;
    interroger();
  });
  // « Poser le tour » garde les poses proposées, au tour en cours, et passe au
  // suivant ; « Écarter » les retire.
  $('propositionReseau').addEventListener('click', (ev) => {
    const bouton = ev.target.closest('button[data-action]');
    if (!bouton) return;
    if (bouton.dataset.action === 'poser' && reseau.apercu) {
      reseau.apercu.poses.forEach((p) => {
        reseau.poses.push({ sort: p.sort, case: p.case, tour: reseau.tour });
      });
      reseau.tour += 1;
    }
    reseau.apercu = null;
    etape = -1;
    interroger();
  });
  $('pasPrecedent').addEventListener('click', () => {
    const n = (chaine() && chaine().etapes.length) || 0;
    etape = etape < 0 ? n - 1 : Math.max(0, etape - 1);
    peindre();
  });
  $('pasSuivant').addEventListener('click', () => {
    const n = (chaine() && chaine().etapes.length) || 0;
    etape = etape < 0 ? 0 : Math.min(n - 1, etape + 1);
    peindre();
  });
  $('pasTout').addEventListener('click', () => { etape = -1; peindre(); });

  function dessinerPalette() {
    $('palettePieges').innerHTML = catalogue().map((c) => {
      const d = c.deplacement ? ` · ${c.deplacement.sens} ${c.deplacement.cases}` : '';
      return `<button type="button" data-sort="${echapper(c.sort)}"
        aria-pressed="${c.sort === reseau.choisi}">
        ${c.dofusdb_id ? `<img src="icons/${c.dofusdb_id}.png" alt="" decoding="async" loading="lazy">` : ''}
        <span>${echapper(c.nom)}</span>
        <span class="cout">${c.pa} PA · ${nb(c.degats[0])}-${nb(c.degats[1])}${
          c.poison ? `, poison ${nb(c.poison[0])}-${nb(c.poison[1])}` : ''}
          · ${c.declenchement} ${pluriel(c.declenchement, 'case')}${d}</span>
      </button>`;
    }).join('');
    $('palettePieges').querySelectorAll('button').forEach((b) => {
      b.addEventListener('click', () => {
        reseau.choisi = b.dataset.sort;
        dessinerPalette();
        rafraichirApercu($('plateauReseau'));
      });
    });
  }

  function peindre() {
    const liste = poses();
    const decl = new Set();
    liste.forEach((p) => (p.declenchement || []).forEach((c) => decl.add(`${c[0]},${c[1]}`)));
    const t = trajetDe(chaine(), reseau.entree, etape);

    // Un groupe par piège. Les poses que le générateur propose viennent après
    // celles du réseau, et leur pastille porte déjà leur rang dans l'ordre de
    // pose. À l'essai, posé au tour affiché, ou posé avant : trois aspects.
    const groupes = liste.map((p, i) => groupePiege(p, i, i >= envoyees ? ' proposee'
      : (p.tour || 1) === reseau.tour ? ' du-tour' : ' anterieur', i + 1)).join('');

    // Les jetons restent sur les cases de départ ; les trajets disent où
    // chacun va. Les numéros sont ceux de la pile.
    let jetons = reseau.lanceur ? jeton(reseau.lanceur, 'lanceur', 'S') : '';
    // La case d'entrée, toujours marquée : c'est en y entrant que la cible
    // prend le réseau.
    if (reseau.entree) jetons += jeton(reseau.entree, 'entree', 'E');
    reseau.allies.forEach((c, i) => { jetons += jeton(c, 'allie', `A${i + 1}`); });
    // Les autres ennemis, en Poutch, gardent leur numéro, celui de la pile.
    jetons += poutchs(reseau.ennemis.map((c, i) => ({ case: c, rang: `E${i + 1}` })));

    dessiner($('plateauReseau'), {
      plateau,
      apercu,
      case: (x, y) => {
        const classes = [];
        const estSram = memeCase(reseau.lanceur, [x, y]);
        const estEntree = memeCase(reseau.entree, [x, y]);
        const estArrivee = t.arrivee && memeCase(t.arrivee, [x, y]) && !estEntree;
        const estMur = reseau.obstacles.some((c) => memeCase(c, [x, y]));
        const allie = reseau.allies.findIndex((c) => memeCase(c, [x, y]));
        const ennemi = reseau.ennemis.findIndex((c) => memeCase(c, [x, y]));
        const i = liste.findIndex((p) => memeCase(p.case, [x, y]));
        if (estSram) classes.push('lanceur');
        if (estEntree) classes.push('entree');
        if (estArrivee) classes.push('arrivee-case');
        if (estMur) classes.push('mur');
        if (i >= 0) classes.push('piege');
        return {
          classes,
          titre: i >= 0 ? `${echapper(liste[i].nom)}, posé en ${i + 1}${i === 0 ? 'er' : 'e'}`
            : estSram ? 'Vous'
            : estEntree ? 'Entrée du réseau'
            : allie >= 0 ? `Allié ${allie + 1}`
            : ennemi >= 0 ? `Ennemi ${ennemi + 1}`
            : estArrivee ? 'Où la cible finit'
            : estMur ? 'Obstacle'
            : decl.has(`${x},${y}`) ? 'Déclenche un piège'
            : 'Case libre',
        };
      },
      calques: groupes + t.chemin,
      jetons,
      surClic: cliquer,
      surSurvol: surSurvolDesPieges($('plateauReseau'), poses),
    });

    // Les PA de chaque tour, poses acceptées seulement : une pose refusée ne
    // coûte rien, et celles que le générateur propose ne sont pas jouées.
    const posees = liste.slice(0, envoyees);
    const parTour = new Map();
    posees.filter((p) => !p.refus).forEach((p) => {
      parTour.set(p.tour, (parTour.get(p.tour) || 0) + (p.pa || 0));
    });
    const tours = [...parTour.entries()].sort((a, b) => a[0] - b[0])
      .map(([n, pa]) => `tour ${n} : ${pa}/${reseau.pa} PA`).join(' · ');
    const refuses = posees.filter((p) => p.refus);
    $('etatReseau').innerHTML = posees.length
      ? `${posees.length} ${pluriel(posees.length, 'piège')} ${pluriel(posees.length, 'posé')}`
        + (tours ? ` · ${tours}` : '')
        + (refuses.length
          ? ` · <span class="alerte">${refuses.length} refusé${refuses.length > 1 ? 's' : ''} : `
            + `${refuses.map((p) => `${echapper(p.nom)} (${echapper(p.refus)})`).join(', ')}</span>`
          : '')
      : 'Choisissez un piège dans la palette, puis cliquez une case.';
    if (!estUnSram(etat)) {
      $('etatReseau').innerHTML += ' Dégâts d\'un Sram niveau 200 sans équipement.';
    }
    if (!reseau.lanceur && posees.length) {
      $('etatReseau').innerHTML += ' Placez le Sram pour juger la portée des poses.';
    }
    if (bilan) $('etatReseau').innerHTML += echapper(bilan);
    $('tourCourant').textContent = `Tour ${reseau.tour}`;
    $('tourPrecedent').disabled = reseau.tour <= 1;

    dessinerProposition();
    dessinerArrivee(t.arrivee);
    dessinerPile();
  }

  /// Ce que le générateur propose pour le tour en cours, et de quoi le poser.
  function dessinerProposition() {
    const bloc = $('propositionReseau');
    const a = reseau.apercu;
    bloc.hidden = !a;
    if (!a) return;
    if (!a.poses || !a.poses.length) {
      bloc.className = 'bandeau souci';
      bloc.textContent = a.raison
        || `Aucune pose n'ajoute de dégâts avec les ${a.pa_restants} PA qui restent au tour ${a.tour}.`;
      return;
    }
    bloc.className = 'bandeau bon';
    const n = a.poses.length;
    const parPa = Math.round((a.gain[0] + a.gain[1]) / 2 / a.pa);
    bloc.innerHTML = `Tour ${a.tour} : <strong>${n} ${pluriel(n, 'piège')}</strong>, `
      + `${a.pa} PA sur ${a.pa_restants} · <strong>+ ${nb(a.gain[0])} à ${nb(a.gain[1])}</strong> dégâts`
      + ` (${nb(parPa)} par PA)`
      + ' <button type="button" class="action" data-action="poser">Poser le tour</button>'
      + ' <button type="button" class="action discret" data-action="ecarter">Écarter</button>';
  }

  /// Ce que la page envoie : le réseau au sol au tour affiché, et au besoin
  /// les poses proposées, à l'essai à ce tour.
  function corps(avecApercu) {
    const essai = avecApercu && reseau.apercu && reseau.apercu.poses
      ? reseau.apercu.poses.map((p) => ({ sort: p.sort, case: p.case, tour: reseau.tour }))
      : [];
    const sol = auSol();
    envoyees = sol.length;
    return {
      ...leSram(etat),
      ...pourLeServeur(plateau),
      lanceur: reseau.lanceur,
      poses: sol.concat(essai),
      entree: reseau.entree,
      obstacles: reseau.obstacles,
      allies: reseau.allies,
      ennemis: reseau.ennemis,
      chakra: reseau.chakra,
      pa: reseau.pa,
      tour: reseau.tour,
    };
  }

  async function proposer() {
    const bouton = $('proposerTour');
    bouton.disabled = true;
    bouton.textContent = 'Recherche…';
    reseau.apercu = null;
    try {
      // ⚠️ LES PIÈGES DES TOURS D'APRÈS GARDENT LEUR CASE. Ils ne sont pas
      // encore au sol au tour affiché, mais le générateur ne doit pas la
      // proposer : posé, le tour les ferait refuser.
      const reservees = reseau.poses.filter((p) => (p.tour || 1) > reseau.tour).map((p) => p.case);
      const r = await appeler('reseau', { ...corps(false), proposer: true, reservees });
      reseau.apercu = r.proposition || null;
    } catch (e) {
      reseau.apercu = { poses: [], raison: e.message };
    }
    bouton.disabled = false;
    bouton.textContent = 'Proposer le tour';
    etape = -1;
    await interroger();
  }

  function dessinerArrivee(arrivee) {
    const bloc = $('arrivee');
    const ch = chaine();
    const depart = reseau.entree;
    if (!depart) {
      bloc.className = 'bandeau souci';
      bloc.textContent = "Indiquez la case d'entrée de l'ennemi pour dérouler la chaîne.";
      return;
    }
    bloc.className = 'bandeau bon';
    const bouge = arrivee && !memeCase(arrivee, depart);
    // ⚠️ L'ÉCART EST LA DISTANCE NETTE entre le départ et l'arrivée, pas la
    // longueur du trajet : la cible peut parcourir sept cases et finir à une
    // seule de son point de départ.
    const ecart = bouge ? distance(arrivee, depart) : 0;
    // Combien d'ennemis la chaîne touche : les pièges ne frappent qu'eux.
    const touches = ch && ch.entites
      ? ch.entites.filter((e) => e.camp === 'ennemi' && e.degats[1] > 0).length : 0;
    bloc.innerHTML = (bouge
      ? `La cible finit <strong>à ${ecart} ${pluriel(ecart, 'case')}</strong> de son entrée`
      : 'La cible <strong>ne bouge pas</strong>')
      + (ch ? ` · <strong>${nb(ch.total[0])} à ${nb(ch.total[1])}</strong> dégâts`
          + (touches > 1 ? ` sur ${touches} ennemis` : '')
          + (etape >= 0 ? ' <span class="aide">(à l\'étape affichée)</span>' : '')
        : '');
  }

  function dessinerPile() {
    const ch = chaine();
    $('pileBloc').hidden = !ch;
    if (!ch) return;
    $('pileListe').innerHTML = lignesDeLaPile(ch, catalogue(), reponse && reponse.declencheur, etape);
    $('pileResume').textContent = resumeDeLaPile(ch, etape);
  }

  async function interroger() {
    try {
      reponse = await appeler('reseau', corps(true));
      if (!reseau.choisi && (reponse.catalogue || []).length) {
        reseau.choisi = reponse.catalogue[0].sort;
      }
    } catch (e) {
      reponse = null;
      $('etatReseau').innerHTML = `<span class="alerte">${echapper(e.message)}</span>`;
    }
    dessinerPalette();
    peindre();
  }

  await interroger();
}
