// Rotation : quels sorts lancer, dans quel ordre.

import { COULEUR_ELEMENT, echapper, majuscule, nb, pluriel } from '../commun/format.js';
import { parPA, rythme, selonCibles } from '../commun/graphique.js';
import { aMesurer, CRANS, dessiner as dessinerLesValeurs, lire as lireLesValeurs, orientationDu } from '../commun/valeurs.js';
import { brancherRepli, classeRepli, replier, titreRepliable } from '../commun/repli.js';
import { borne, decrire, DOMAINES, ELEMENTS, fixe, neutres, versMoteur } from '../commun/resistances.js';
import { cochesParDefaut, MAX_SORTS, sortsDuBuild } from '../commun/deck.js';
import * as discord from '../commun/discord.js';
import { ouvrirLeCompagnon, publier as publierPourLeCompagnon } from '../commun/suivi.js';
import { brancherLesReglages, reglages as reglagesDesTouches, signalerLesPrises } from '../commun/touches.js';
import { contenuDuPartage, enregistrer as enregistrerLePartage, nomDuFichier } from '../commun/partage.js';
import { lienDuPartage } from '../commun/lien.js';
import {
  actif as placementActif, brancher as brancherLePlacement, encart as encartPlacement,
  pourLaRequete as placementPourLaRequete,
} from '../commun/placement-rotation.js';
import { garder, reglagesRotation } from '../commun/reglages-rotation.js';

const ROUBLARD = 13;
const NOMS_DES_BOMBES = { feu: 'Explobombes', air: 'Tornabombes', eau: 'Bombes à Eau', terre: 'Sismobombes' };
const ROMAINS = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X', 'XI', 'XII', 'XIII', 'XIV', 'XV'];

export async function rendre(cible, etat, { appeler }) {
  /// Le même plafond que le damier, donné par le moteur.
  const MAX_ENNEMIS = (etat.classes && etat.classes.limites
    && etat.classes.limites.ennemis) || 1;
  const classe = (etat.classes && etat.classes.classes || [])
    .find((c) => c.id === (etat.resolu && etat.resolu.class));
  /// L'arme du build, un sort de plus au deck, avec l'icône de l'objet.
  const arme = (etat.resolu && etat.resolu.arme) || null;
  /// Les sorts que des objets du build ajoutent à la barre, avec l'icône de leur
  /// objet.
  const sortsDObjets = (etat.resolu && etat.resolu.sorts_d_objets) || [];
  /// Les sorts du build, avec ce que ses objets de classe leur changent : leur
  /// coût en PA d'abord, que le deck affiche et classe.
  const sorts = sortsDuBuild(classe, etat.resolu);
  /// L'image d'un sort ou de l'arme, à la taille demandée.
  const imageDe = (x, taille) => {
    const dim = taille ? ` width="${taille}" height="${taille}"` : '';
    if (x.icone_objet) return `<img src="objets/${x.icone_objet}.webp" alt=""${dim} decoding="async">`;
    return x.dofusdb_id ? `<img src="icons/${x.dofusdb_id}.png" alt=""${dim} decoding="async" loading="lazy">` : '';
  };
  /// Les mécaniques coûteuses de la classe, comptées d'office : elles n'affichent
  /// que la case du calcul exhaustif, seule recherche qui les essaie sur tous les
  /// sorts.
  const couteuses = (classe && classe.couteuses) || [];

  const reglages = reglagesRotation(etat);
  if (reglages.cibles > MAX_ENNEMIS) reglages.cibles = MAX_ENNEMIS;
  const res = () => reglages.resistances || (reglages.resistances = neutres());
  const MODES_DE_RESISTANCE = [['aucune', 'Aucune'], ['uniformes', 'Uniformes'], ['elements', 'Par élément']];
  /// La position écarte du deck ce qui ne peut pas frapper de là.
  const POSITIONS = [['libre', 'Au choix'], ['distance', 'Distance'], ['melee', 'Mêlée']];
  const AIDE_POSITION = {
    libre: '',
    distance: "Les sorts et l'arme qui ne frappent qu'au contact sortent de la rotation.",
    melee: 'Les sorts qui exigent 2 cases ou plus sortent de la rotation.',
  };
  const position = () => reglages.position || 'libre';
  /// Les réglages que la CLASSE fait déclarer, découverts dans ses règles :
  /// la vie restante du Sacrieur, la poussée du Steamer, le glyphe-aura du
  /// Féca. Chacun part de son défaut, et garde ce que le joueur y a mis.
  // Après la déclaration de `reglages`. Le deck est celui d'une classe : un build
  // d'une autre classe repart de ses défauts.
  if (reglages.classe !== (classe && classe.id)) {
    reglages.classe = classe && classe.id;
    reglages.coches = null;
    reglages.mur = null;
  }
  const declares = (classe && classe.etats_declares) || [];
  /// Les bonus de Dofus qui dépendent du combat, ceux du build chargé : une case
  /// décochée les écarte du calcul. L'écart vit dans `etat`, que KrozZone suit
  /// aussi.
  const bonusDofus = (etat.resolu && etat.resolu.bonus_conditionnels) || [];
  const ecartes = () => etat.bonusEcartes || [];
  /// Ceux que le joueur ajoute, leur case décochée d'office (l'Œil du
  /// Cauchemar, qu'un ennemi déclenche).
  const inclus = () => etat.bonusInclus || [];
  const cochee = (b) => (b.coche === false ? inclus().includes(b.nom) : !ecartes().includes(b.nom));
  /// Le mur de bombes que Plombage redéclenche, pour le Roublard : celui que le
  /// joueur règle ici, sinon le premier élément dont KrozBoom a posé deux bombes,
  /// sinon deux Explobombes tout juste posées.
  const estRoublard = Boolean(classe && classe.id === ROUBLARD);
  const murDeKrozBoom = () => {
    const bombes = (etat.boom && etat.boom.bombes) || [];
    const paire = bombes.find((b) => bombes.filter((a) => a.element === b.element).length >= 2);
    if (!paire) return { element: 'feu', combos: [1, 1] };
    return {
      element: paire.element,
      combos: bombes.filter((b) => b.element === paire.element).slice(0, 3).map((b) => b.combo),
    };
  };
  const mur = () => reglages.mur || murDeKrozBoom();
  if (!reglages.etats) reglages.etats = {};
  for (const d of declares) {
    if (!(d.id in reglages.etats)) reglages.etats[d.id] = Number(d.defaut) || 0;
  }
  if (!reglages.coches) {
    reglages.coches = cochesParDefaut(sorts, etat.resolu);
  }
  let resultat = null;
  // Le critique du build que le calcul a pris, Dofus compris : c'est lui qui
  // décide si tout est compté en coup critique.
  let critiqueDuResultat = null;
  // La requête qui a donné la rotation affichée : les conseils la rejouent
  // telle quelle, même si les réglages ont changé depuis.
  let chargeDuResultat = null;
  let conseilOuvert = null;
  let balayage = null;
  // Ce que vaut chaque caractéristique pour la rotation affichée, une fois
  // mesuré.
  let valeurs = null;
  let enCours = false;
  let perime = false;

  // La liste des sorts s'ouvre à chaque affichage : sans résultat, repliée,
  // la page n'aurait rien à montrer.
  if (etat.replies) etat.replies.sorts = false;

  cible.innerHTML = `
    <section class="outil">
      <h2>Rotation</h2>
      <p class="sous">Cochez les sorts à inclure dans la rotation.</p>

      <div class="vue-rotation">
        <div>
          <div class="carte${classeRepli(etat, 'sorts')}">
            ${titreRepliable(etat, 'sorts', 'Sorts', ' <span id="compteurSorts" class="aide"></span>')}
            <div class="barre-outils">
              <button type="button" class="action discret" id="toutDecocher">Tout décocher</button>
              <button type="button" class="action discret" id="orientation">Limiter à l'orientation</button>
            </div>
            <div class="sorts" id="listeSorts"></div>
          </div>
          ${encartPlacement(etat)}
          <div id="detailRotation" style="margin-top:1rem"></div>
          <div class="carte" id="carteInvocations" style="margin-top:1rem" hidden></div>
        </div>

        <aside class="panneau" id="panneau">
          <button type="button" class="action" id="calculer">Calculer</button>
          <button type="button" class="action discret" id="annuler" hidden>Annuler</button>
          <div class="barre" id="barreCalcul" hidden><span id="barreCalculJauge"></span></div>
          <p class="aide" id="etatCalcul"></p>
          <!-- L'essentiel d'abord, le reste replié. Le compteur dit combien de
               réglages repliés diffèrent du défaut : un réglage caché ne
               fausse pas le total en silence. -->
          <div class="reglages">
            <div><label for="horizon">Tours</label>
              <input type="number" id="horizon" min="1" max="15" value="${reglages.horizon}"></div>
            <div><label for="cibles">Ennemis</label>
              <input type="number" id="cibles" min="1" max="${MAX_ENNEMIS}" value="${reglages.cibles}"></div>
          </div>
          <p class="libelle-groupe">Votre position</p>
          <div class="barre-outils">${POSITIONS.map(([v, t]) => `
            <label class="segment"><input type="radio" name="position" value="${v}"${position() === v ? ' checked' : ''}> ${t}</label>`).join('')}
          </div>
          <p class="aide" id="aidePosition"${AIDE_POSITION[position()] ? '' : ' hidden'}>${AIDE_POSITION[position()]}</p>
          <p class="libelle-groupe">Résistances de la cible</p>
          <div class="barre-outils">${MODES_DE_RESISTANCE.map(([v, t]) => `
            <label class="segment"><input type="radio" name="resistances" value="${v}"${res().mode === v ? ' checked' : ''}> ${t}</label>`).join('')}
          </div>
          <div class="reglages" id="resUniformes"${res().mode === 'uniformes' ? '' : ' hidden'}>
            <div><label for="resUniforme">% à tous les éléments</label>
              <input type="number" id="resUniforme" min="-100" max="100" value="${res().uniforme}"></div>
          </div>
          <div class="resistances-elements" id="resElements"${res().mode === 'elements' ? '' : ' hidden'}>${ELEMENTS.map(([e, nom]) => `
            <label for="res-${e}">${nom}<input type="number" id="res-${e}" data-res-element="${e}" min="-100" max="100"
              value="${res().elements[e] || 0}"></label>`).join('')}
          </div>
          <p class="aide" id="aideResistances"${res().mode === 'aucune' ? ' hidden' : ''}>En %, pour tous les ennemis.</p>
          <p class="libelle-groupe">Invocations</p>
          <label class="bascule" for="invocationsDeClasse" id="caseInvocationsDeClasse" hidden>
            <input type="checkbox" id="invocationsDeClasse" ${reglages.invocationsDeClasse !== false ? 'checked' : ''}>
            <span>De la classe</span>
          </label>
          <label class="bascule" for="invocationsCommunes">
            <input type="checkbox" id="invocationsCommunes" ${reglages.invocationsCommunes ? 'checked' : ''}>
            <span>Communes (Arakne, Chaferfu, Cawotte)</span>
          </label>
          <p class="aide">Cochées, elles jouent chaque tour jusqu'au dernier, sans jamais mourir.</p>
          <p class="libelle-groupe">Poussées</p>
          <label class="bascule" for="pousseesBloquees">
            <input type="checkbox" id="pousseesBloquees" ${reglages.pousseesBloquees ? 'checked' : ''}>
            <span>Inclure les dommages de poussée</span>
          </label>
          <p class="aide" id="aidePoussees">Vos poussées butent toutes contre un obstacle.</p>
          <details class="plus-loin" id="plusLoin" ${etat.plusLoinOuvert ? 'open' : ''}>
            <summary>Pour aller plus loin <span class="modifies" id="reglagesModifies"></span></summary>
            <div class="reglages">
              <div><label for="pmDepenses">PM dépensés</label>
                <input type="number" id="pmDepenses" min="0" max="12" value="${reglages.pmDepenses}"></div>
              <div><label for="etalement">Cases entre les ennemis touchés</label>
                <input type="number" id="etalement" min="0" max="4" value="${reglages.etalement}"></div>
              <p class="aide reglage-aide">À 0, tous sont au centre de la zone ; les sorts dégressifs perdent de
                la puissance avec la distance.</p>
              ${arme ? `<div><label for="maitrise">Maîtrise d'arme</label>
                <select id="maitrise">
                  ${[[0, 'Aucune'], [300, 'Normale (300)'], [360, 'Critique (360)']]
                    .map(([v, t]) => `<option value="${v}"${(reglages.maitrise ?? 300) === v ? ' selected' : ''}>${t}</option>`).join('')}
                </select></div>` : ''}
            </div>
            <h4 class="groupe">La cible</h4>
            <p class="libelle-groupe">Résistances fixes</p>
            <div class="resistances-elements">${ELEMENTS.map(([e, nom]) => `
              <label for="fixe-${e}">${nom}<input type="number" id="fixe-${e}" data-res-fixe="${e}"
                value="${(res().fixes && res().fixes[e]) || 0}"></label>`).join('')}
            </div>
            <div class="reglages">
              <div><label for="res-critiques">Résist. critiques</label>
                <input type="number" id="res-critiques" value="${res().critiques || 0}"></div>${DOMAINES.filter(([d]) => d !== 'armes' || arme).map(([d, nom]) => `
              <div><label for="res-${d}">% Résist. ${nom}</label>
                <input type="number" id="res-${d}" data-res-domaine="${d}" min="-100" max="100" value="${res()[d] || 0}"></div>`).join('')}
            </div>
            ${declares.length || estRoublard ? '<h4 class="groupe">Votre combat</h4>' : ''}
            ${declares.filter((d) => Number(d.max) > 1).length ? `<div class="reglages">${
              declares.filter((d) => Number(d.max) > 1).map((d) => `
              <div><label for="etat-${echapper(d.id)}">${echapper(d.label)}</label>
                <input type="number" id="etat-${echapper(d.id)}" data-etat="${echapper(d.id)}"
                  min="0" max="${Number(d.max)}" value="${reglages.etats[d.id]}"></div>`).join('')}</div>` : ''}
            ${declares.filter((d) => Number(d.max) === 1).map((d) => `
            <label class="bascule" for="etat-${echapper(d.id)}">
              <input type="checkbox" id="etat-${echapper(d.id)}" data-etat="${echapper(d.id)}"
                ${reglages.etats[d.id] ? 'checked' : ''}>
              <span>${echapper(d.label)}</span>
            </label>`).join('')}
            ${estRoublard ? `
            <div class="mur-bombes">
              <label for="murElement">Mur de bombes de la cible</label>
              <select id="murElement">
                ${Object.entries(NOMS_DES_BOMBES).map(([id, nom]) => `<option value="${id}"${mur().element === id ? ' selected' : ''}>${nom}</option>`).join('')}
              </select>
              ${mur().combos.map((c, i) => `<select data-mur-combo="${i}" aria-label="Combo de la bombe ${i + 1}">
                ${ROMAINS.map((r, k) => `<option value="${k + 1}"${c === k + 1 ? ' selected' : ''}>${r}</option>`).join('')}
              </select>`).join('')}
              <p class="aide" id="hypothesesMur"></p>
            </div>` : ''}
            ${bonusDofus.length ? `
            <h4 class="groupe">Bonus d'équipement tenus</h4>
            <div class="bonus-dofus">
              ${bonusDofus.map((b, i) => `
                <label class="bascule" for="dofus-${i}">
                  <input type="checkbox" id="dofus-${i}" data-bonus="${echapper(b.nom)}"
                    data-coche="${b.coche === false ? 'non' : 'oui'}" ${cochee(b) ? 'checked' : ''}>
                  <span>${echapper(b.nom)}, ${echapper(b.effet)}</span>
                </label>
                <p class="aide palier-aide">${echapper(majuscule(b.condition))}.</p>`).join('')}
            </div>` : ''}
            ${couteuses.length ? `
            <h4 class="groupe">Calcul</h4>
            <label class="bascule" for="exhaustif">
              <input type="checkbox" id="exhaustif" ${reglages.exhaustif ? 'checked' : ''}>
              <span>Calcul exhaustif</span>
            </label>
            <p class="aide palier-aide">Plus long, rarement meilleur.</p>` : ''}
          </details>
          <div id="resumeRotation"></div>
        </aside>
      </div>

      <!-- ⚠️ LES GRAPHIQUES NE TIENNENT PAS DANS LA COLONNE. Leur dessin fait
           700 unités de large avec des étiquettes de 10 : ramené aux 350 px de
           la colonne de gauche, l'axe se rend en caractères de cinq pixels et
           ne se lit plus. Ils prennent donc toute la largeur, sous les deux
           colonnes, là où ils ont la place de dire quelque chose. -->
      <div id="graphiques"></div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);

  /// Les sorts vont par paires de variantes, et le jeu n'en laisse équiper qu'une :
  /// `variant_group` porte l'appariement.
  const paires = (() => {
    const groupes = new Map();
    for (const s of sorts) {
      const clef = s.variant_group != null ? `g${s.variant_group}` : `s${s.id}`;
      if (!groupes.has(clef)) groupes.set(clef, []);
      groupes.get(clef).push(s);
    }
    return [...groupes.values()];
  })();

  /// Les paires dont les DEUX variantes sont cochées. C'est la seule erreur que
  /// cet écran peut voir sans lancer le calcul.
  const enConflit = () => paires.filter(
    (g) => g.length > 1 && g.every((s) => reglages.coches.has(s.id)),
  );

  function dessinerSorts() {
    // Ce qui borne le sort, nommé sur sa ligne : rien à titrer en colonne.
    const bornes = (s) => [
      `${s.ap} PA`,
      s.cooldown ? `relance ${s.cooldown} ${pluriel(s.cooldown, 'tour')}` : '',
      s.casts_per_turn ? `${s.casts_per_turn} par tour` : '',
      s.casts_per_target ? `${s.casts_per_target} par cible` : '',
    ].filter(Boolean).join(' · ');
    const carte = (s) => `
      <label class="sort${reglages.coches.has(s.id) ? ' coche' : ''}" data-sort="${echapper(s.id)}">
        <input type="checkbox" value="${echapper(s.id)}" ${reglages.coches.has(s.id) ? 'checked' : ''}>
        ${imageDe(s) || '<span class="vide"></span>'}
        <span class="quoi"><b>${echapper(s.name)}</b><small>${bornes(s)}</small></span>
      </label>`;

    // Un encart par emplacement, ses variantes dedans, comme la fiche du build.
    $('listeSorts').innerHTML = paires.map((g) => {
      const conflit = g.length > 1 && g.every((s) => reglages.coches.has(s.id));
      const actif = g.some((s) => reglages.coches.has(s.id));
      return `<div class="paire${actif ? ' actif' : ''}${conflit ? ' conflit' : ''}"
        data-groupe="${echapper(g[0].variant_group != null ? String(g[0].variant_group) : g[0].id)}"
        ${conflit ? `title="Décochez ${echapper(g.map((s) => s.name).join(' ou '))}."` : ''}
        >${g.map(carte).join('')}</div>`;
    }).join('');

    $('listeSorts').querySelectorAll('input').forEach((i) => {
      i.addEventListener('change', () => {
        if (i.checked) reglages.coches.add(i.value); else reglages.coches.delete(i.value);
        i.closest('label').classList.toggle('coche', i.checked);
        // La paire entière change d'état, pas la case : c'est la RELATION entre
        // les deux qui est fautive.
        const paire = i.closest('.paire');
        const cases = [...paire.querySelectorAll('input')];
        paire.classList.toggle('conflit', cases.length > 1 && cases.every((c) => c.checked));
        paire.classList.toggle('actif', cases.some((c) => c.checked));
        majCompteur();
        marquerPerime();
        dessinerInvocations();
      });
    });
    dessinerInvocations();
    majCompteur();
  }

  function majCompteur() {
    const n = reglages.coches.size;
    const trop = Math.max(0, n - MAX_SORTS);
    const conflits = enConflit();
    $('compteurSorts').innerHTML = trop
      ? `· <span class="alerte">${n} cochés, ${trop} de trop</span>`
      : `· ${n} coché${n > 1 ? 's' : ''}`
        + (conflits.length
          ? `, <span class="alerte">${conflits.length} variante${conflits.length > 1 ? 's' : ''} en trop</span>`
          : '');
    // Refuser tôt. Une paire de variantes cochée des deux côtés ferme le bouton : la
    // rotation serait injouable.
    $('calculer').disabled = enCours || n === 0 || trop > 0 || conflits.length > 0;
    direPourquoiFerme();
  }

  /// Un chiffre affiché dit s'il correspond encore aux réglages, et le bouton grisé
  /// dit pourquoi il l'est.
  function direPourquoiFerme() {
    const n = reglages.coches.size;
    const conflits = enConflit();
    const etat = $('etatCalcul');
    if (enCours) return;
    if (conflits.length) {
      etat.innerHTML = `<span class="alerte">Décochez `
        + `${conflits.map((g) => g.map((x) => echapper(x.name)).join(' ou ')).join(', ')}.</span>`;
    } else if (n === 0) {
      etat.textContent = 'Cochez au moins un sort.';
    } else if (n > MAX_SORTS) {
      etat.innerHTML = `<span class="alerte">Gardez ${MAX_SORTS} sorts au plus.</span>`;
    } else if (!resultat) {
      etat.textContent = '';
    }
  }

  /// Combien de réglages de « Pour aller plus loin » diffèrent de leur défaut.
  function majModifies() {
    const n = [
      reglages.pmDepenses !== 0,
      reglages.etalement !== 0,
      Boolean(arme) && (reglages.maitrise ?? 300) !== 300,
      Boolean(reglages.exhaustif),
      estRoublard && Boolean(reglages.mur),
      ...DOMAINES.filter(([d]) => d !== 'armes' || arme).map(([d]) => Boolean(res()[d])),
      ...ELEMENTS.map(([e]) => Boolean(res().fixes && res().fixes[e])),
      Boolean(res().critiques),
      ...declares.map((d) => Number(reglages.etats[d.id]) !== (Number(d.defaut) || 0)),
      ...bonusDofus.map((b) => cochee(b) !== (b.coche !== false)),
    ].filter(Boolean).length;
    $('reglagesModifies').textContent = n ? `${n} ${pluriel(n, 'modifié')}` : '';
  }

  // Le placement : posé sur un plateau, il décide qui chaque sort touche et
  // quelles poussées butent, et le nombre d'ennemis comme les poussées déclarés
  // se taisent.
  const majSelonLePlacement = () => {
    const on = placementActif(etat);
    $('cibles').disabled = on;
    $('cibles').title = on ? 'Le placement décide : posez les ennemis sur le plateau.' : '';
    $('cibles').value = String(on ? Math.max(1, etat.placement.ennemis.length) : reglages.cibles);
    $('pousseesBloquees').disabled = on;
    $('aidePoussees').textContent = on
      ? 'Le placement décide : une poussée compte quand elle bute sur un obstacle, sur vous ou sur un ennemi.'
      : 'Vos poussées butent toutes contre un obstacle.';
  };
  const placementUI = await brancherLePlacement(cible, etat, {
    appeler,
    maxEnnemis: MAX_ENNEMIS,
    change: () => { majSelonLePlacement(); marquerPerime(); },
  });
  majSelonLePlacement();
  // Un lancer survolé dans la rotation : sa zone sur le plateau.
  const survolerUnLancer = (e) => {
    const l = e.target.closest && e.target.closest('.lancer[data-tour]');
    if (!l || !placementActif(etat)) return;
    placementUI.survoler({ sort: l.dataset.sort, tour: Number(l.dataset.tour), lancer: Number(l.dataset.lancer) });
  };
  $('detailRotation').addEventListener('mouseover', survolerUnLancer);
  $('detailRotation').addEventListener('focusin', survolerUnLancer);
  $('detailRotation').addEventListener('mouseleave', () => placementUI.survoler(null));

  function marquerPerime() {
    majModifies();
    garder(reglages);
    if (!resultat) return;
    perime = true;
    $('panneau').classList.add('perime');
    $('calculer').textContent = 'Recalculer';
  }

  $('plusLoin').addEventListener('toggle', () => { etat.plusLoinOuvert = $('plusLoin').open; });
  ['horizon', 'pmDepenses', 'cibles', 'etalement'].forEach((id) => {
    $(id).addEventListener('input', () => {
      reglages[id] = Number($(id).value) || 0;
      marquerPerime();
    });
  });
  $('pousseesBloquees').addEventListener('change', () => {
    reglages.pousseesBloquees = $('pousseesBloquees').checked;
    marquerPerime();
  });
  cible.querySelectorAll('input[name="resistances"]').forEach((r) => {
    r.addEventListener('change', () => {
      res().mode = r.value;
      $('resUniformes').hidden = r.value !== 'uniformes';
      $('resElements').hidden = r.value !== 'elements';
      $('aideResistances').hidden = r.value === 'aucune';
      marquerPerime();
    });
  });
  $('resUniforme').addEventListener('input', () => {
    res().uniforme = borne($('resUniforme').value);
    marquerPerime();
  });
  cible.querySelectorAll('[data-res-element]').forEach((i) => {
    i.addEventListener('input', () => {
      res().elements[i.dataset.resElement] = borne(i.value);
      marquerPerime();
    });
  });
  cible.querySelectorAll('[data-res-fixe]').forEach((i) => {
    i.addEventListener('input', () => {
      if (!res().fixes) res().fixes = {};
      res().fixes[i.dataset.resFixe] = fixe(i.value);
      marquerPerime();
    });
  });
  $('res-critiques').addEventListener('input', () => {
    res().critiques = fixe($('res-critiques').value);
    marquerPerime();
  });
  cible.querySelectorAll('[data-res-domaine]').forEach((i) => {
    i.addEventListener('input', () => {
      res()[i.dataset.resDomaine] = borne(i.value);
      marquerPerime();
    });
  });
  cible.querySelectorAll('input[name="position"]').forEach((r) => {
    r.addEventListener('change', () => {
      reglages.position = r.value;
      $('aidePosition').textContent = AIDE_POSITION[r.value];
      $('aidePosition').hidden = !AIDE_POSITION[r.value];
      marquerPerime();
    });
  });
  if (arme) {
    $('maitrise').addEventListener('change', () => {
      reglages.maitrise = Number($('maitrise').value);
      marquerPerime();
    });
  }
  if (estRoublard) {
    $('murElement').addEventListener('change', () => {
      reglages.mur = { ...mur(), element: $('murElement').value };
      marquerPerime();
    });
    cible.querySelectorAll('select[data-mur-combo]').forEach((s) => {
      s.addEventListener('change', () => {
        const combos = [...mur().combos];
        combos[Number(s.dataset.murCombo)] = Number(s.value);
        reglages.mur = { ...mur(), combos };
        marquerPerime();
      });
    });
  }
  cible.querySelectorAll('input[data-bonus]').forEach((c) => {
    c.addEventListener('change', () => {
      // Une case cochée d'office s'écarte en la décochant ; une case
      // décochée d'office s'ajoute en la cochant.
      if (c.dataset.coche === 'non') {
        const liste = new Set(inclus());
        if (c.checked) liste.add(c.dataset.bonus);
        else liste.delete(c.dataset.bonus);
        etat.bonusInclus = [...liste];
      } else {
        const liste = new Set(ecartes());
        if (c.checked) liste.delete(c.dataset.bonus);
        else liste.add(c.dataset.bonus);
        etat.bonusEcartes = [...liste];
      }
      marquerPerime();
    });
  });
  $('annuler').addEventListener('click', async () => {
    // L'arrêt est coopératif, en quelques dixièmes de seconde : le bouton se ferme
    // tout de suite pour qu'on ne le reclique pas.
    $('annuler').disabled = true;
    $('etatCalcul').textContent = 'Arrêt demandé…';
    try { await appeler('annuler'); } catch (e) { /* le calcul finira de lui-même */ }
  });
  for (const [id, cle] of [['invocationsDeClasse', 'invocationsDeClasse'], ['invocationsCommunes', 'invocationsCommunes']]) {
    $(id).addEventListener('change', () => {
      reglages[cle] = $(id).checked;
      dessinerInvocations();
      marquerPerime();
    });
  }
  if (couteuses.length) {
    $('exhaustif').addEventListener('change', () => {
      reglages.exhaustif = $('exhaustif').checked;
      marquerPerime();
    });
  }
  cible.querySelectorAll('[data-etat]').forEach((i) => {
    const d = declares.find((x) => x.id === i.dataset.etat);
    i.addEventListener(i.type === 'checkbox' ? 'change' : 'input', () => {
      reglages.etats[d.id] = i.type === 'checkbox'
        ? (i.checked ? 1 : 0)
        : Math.max(0, Math.min(Number(d.max), Math.round(Number(i.value) || 0)));
      marquerPerime();
    });
  });
  $('toutDecocher').addEventListener('click', () => {
    reglages.coches.clear(); dessinerSorts(); marquerPerime();
  });
  $('orientation').addEventListener('click', () => {
    reglages.coches = cochesParDefaut(sorts, etat.resolu);
    dessinerSorts(); marquerPerime();
  });

  /// Une seule définition de la charge : la courbe repart des mêmes réglages que
  /// le total.
  const charge = (extra) => ({
    ...etat.build,
    // L'arme ou le sort d'objet d'un build précédent de la même classe ne
    // part pas : ce build-ci ne les porte peut-être pas.
    deck: [...reglages.coches].filter((id) => (id !== 'arme' || arme)
      && (!id.startsWith('sort_d_objet_') || sortsDObjets.some((s) => s.id === id))),
    horizon: reglages.horizon,
    pm_depenses: reglages.pmDepenses,
    maitrise_d_arme: reglages.maitrise ?? 300,
    position: position(),
    poussees_bloquees: Boolean(reglages.pousseesBloquees),
    targets: placementActif(etat) ? Math.max(1, etat.placement.ennemis.length) : reglages.cibles,
    etalement: reglages.etalement,
    exhaustif: reglages.exhaustif,
    invocations_de_classe: reglages.invocationsDeClasse !== false,
    invocations_communes: Boolean(reglages.invocationsCommunes),
    etats_declares: Object.fromEntries(declares.map((d) => [d.id, reglages.etats[d.id]])),
    bonus_ecartes: ecartes(),
    bonus_inclus: inclus(),
    ...(estRoublard ? { mur_de_bombes: mur() } : {}),
    ...(placementActif(etat) ? { placement: placementPourLaRequete(etat) } : {}),
    ...versMoteur(res(), Boolean(arme)),
    ...extra,
  });

  // Un cran au-delà du réglage, avec un plancher à quatre et le plafond du
  // moteur.
  const borneBalayage = () => Math.max(4, Math.min(MAX_ENNEMIS, reglages.cibles + 1));

  $('calculer').addEventListener('click', async () => {
    enCours = true;
    majCompteur();
    const depart = performance.now();
    // Deux rythmes : le chrono bat vite ; l'avancement, demandé au moteur, plus
    // rarement.
    const barre = $('barreCalcul');
    const jauge = $('barreCalculJauge');
    barre.hidden = false;
    // Le bouton grisé ne dit pas au joueur qu'il peut reprendre la main : il
    // laisse la place à « Annuler » le temps du calcul.
    $('calculer').hidden = true;
    $('annuler').hidden = false;
    $('annuler').disabled = false;
    jauge.style.width = '0%';
    let ou = '';
    let erreur = null;
    const chrono = setInterval(() => {
      const t = ((performance.now() - depart) / 1000).toFixed(1);
      $('etatCalcul').textContent = ou ? `Calcul, ${t} s, ${ou}` : `Calcul, ${t} s…`;
    }, 100);
    const suivi = setInterval(async () => {
      try {
        const a = await appeler('avancement');
        if (!a.running) return;
        jauge.style.width = `${Math.round(Math.min(a.fraction, 0.99) * 100)}%`;
        ou = `tour ${a.turn} sur ${a.turns}`;
      } catch (e) { /* le calcul prime : une lecture ratée ne l'interrompt pas */ }
    }, 250);
    try {
      const demande = charge();
      const d = await appeler('rotation', demande);
      resultat = d.rotation;
      critiqueDuResultat = d.build ? d.build.crit : null;
      chargeDuResultat = demande;
      conseilOuvert = null;
      balayage = null;
      valeurs = null;
      perime = false;
      $('panneau').classList.remove('perime');
      $('calculer').textContent = 'Recalculer';
      $('etatCalcul').textContent = '';
      dessinerResume();
      dessinerDetail();
      dessinerInvocations();
      placementUI.montrer(resultat);
      if ($('hypothesesMur')) {
        $('hypothesesMur').innerHTML = hypotheses('mur').map((m) => `${echapper(majuscule(m))}.`).join('<br>');
      }
      // Le résultat remonte sous la liste, repliée à son bandeau ; un clic sur
      // le bandeau la rouvre.
      replier(cible, etat, 'sorts');
      $('listeSorts').closest('.carte').scrollIntoView({ block: 'nearest' });
    } catch (e) {
      erreur = e.message === 'calcul annulé'
        ? 'Calcul annulé.'
        : `<span class="alerte">${echapper(e.message)}</span>`;
    } finally {
      clearInterval(chrono);
      clearInterval(suivi);
      barre.hidden = true;
      $('annuler').hidden = true;
      $('calculer').hidden = false;
      enCours = false;
      majCompteur();
      // Après le compteur, qui vide la ligne tant qu'aucune rotation n'est
      // affichée : posée avant, l'erreur disparaissait aussitôt.
      if (erreur) $('etatCalcul').innerHTML = erreur;
    }
  });

  /// Les hypothèses du calcul, rangées là où elles expliquent quelque chose :
  /// sous le tour 1, dans l'encart des invocations, sous le mur de bombes ; les
  /// autres, sous le total.
  const LIEUX = { 'Tour 1': 'tour1', Invocations: 'invocations', Tourelles: 'invocations', Bombes: 'mur' };
  const hypotheses = (lieu) => ((resultat && resultat.remarques_par_sujet) || [])
    .filter((g) => (LIEUX[g.sujet] || 'resultat') === lieu)
    .flatMap((g) => g.lignes);

  function dessinerResume() {
    const r = resultat;
    const st = r.steady || {};
    // Dès le seuil du critique sûr au build, le moteur compte en critique tout coup
    // qui peut critiquer : la règle se dit sous le total qu'elle fait monter.
    const certain = etat.classes && etat.classes.limites && etat.classes.limites.critique_certain;
    const toutCritique = certain != null && critiqueDuResultat != null && critiqueDuResultat >= certain;
    const contre = decrire(chargeDuResultat);
    // « Résultat » : « Rotation » est le nom de l'encart des tours, à gauche.
    $('resumeRotation').innerHTML = `
      <h3 style="margin-top:.9rem">Résultat</h3>
      <div class="chiffre">${nb(r.total)}</div>
      <div class="sous-chiffre">dégâts sur ${r.turns.length} ${pluriel(r.turns.length, 'tour')} · ${
        nb(r.per_turn)} par tour</div>
      ${toutCritique ? `<p class="aide">Dès ${certain} de critique, tous les sorts sont comptés en coup critique.</p>` : ''}
      <dl>
        ${contre ? `<dt>Contre</dt><dd>${echapper(contre)}</dd>` : ''}
        ${st.cycle_length ? `<dt title="La rotation se répète tous les ${st.cycle_length} ${
          pluriel(st.cycle_length, 'tour')}">Sur la durée</dt><dd>${nb(st.per_turn)} par tour</dd>` : ''}
        ${r.ap_wasted > 0 ? `<dt>PA perdus</dt><dd class="alerte">${r.ap_wasted}</dd>` : ''}
      </dl>
      ${st.too_large ? '<p class="aide">Moyenne sur la durée non calculée avec autant de sorts.</p>' : ''}
      ${hypotheses('resultat').length ? `<details class="hypotheses"><summary>${hypotheses('resultat').length} ${
        pluriel(hypotheses('resultat').length, 'hypothèse')} du calcul</summary><ul>${hypotheses('resultat')
        .map((m) => `<li>${echapper(majuscule(m))}.</li>`).join('')}</ul></details>` : ''}
      <div class="copie-discord">
        <button type="button" class="action discret" id="copierDiscord">Copier pour Discord</button>
        <button type="button" class="action discret" id="partager" aria-expanded="false"
          aria-controls="menuPartage">Partager</button>
        <div class="pop" id="menuPartage" hidden>
          ${etat.resolu && etat.resolu.partage ? `<button type="button" id="copierLien">Copier le lien<small>Le build, les
            réglages et la rotation</small></button>` : ''}
          <button type="button" id="enregistrerPartage">Enregistrer le fichier<small>${echapper(nomDuFichier(etat))}</small></button>
        </div>
        <button type="button" class="action discret" id="ouvrirCompagnon">Compagnon de combat</button>
        <span class="copie" id="copieDiscord" role="status"></span>
      </div>
      ${reglagesDesTouches()}`;
    // Le compagnon suit chaque nouveau résultat, ouvert ou non.
    publierPourLeCompagnon(r, nomDuBuild(), chargeDuResultat);
    brancherLesReglages($('touchesCompagnon'));
    $('ouvrirCompagnon').addEventListener('click', () => {
      ouvrirLeCompagnon(appeler).then((ouvert) => {
        signalerLesPrises($('touchesCompagnon'), ouvert && ouvert.prises);
      }).catch((e) => {
        $('copieDiscord').className = 'copie refus';
        $('copieDiscord').textContent = e.message;
      });
    });
    // Le lien de la rotation, celui que Partager copie et que le message pour
    // Discord porte ; aucun sans le build résolu pour le partage.
    const lienDeLaRotation = () => (etat.resolu && etat.resolu.partage
      ? lienDuPartage(etat.resolu.partage, reglages, sorts,
        { ecartes: etat.bonusEcartes || [], inclus: etat.bonusInclus || [] }, declares, placementPourLaRequete(etat))
      : null);
    $('copierDiscord').addEventListener('click', async () => {
      const texte = discord.rotation({
        r,
        demande: chargeDuResultat,
        build: nomDuBuild(),
        version: await discord.version(appeler),
        lien: etat.lienDuBuild,
        partage: lienDeLaRotation(),
      });
      const ok = await discord.copier(texte);
      $('copieDiscord').className = `copie ${ok ? 'ok' : 'refus'}`;
      $('copieDiscord').textContent = ok ? 'Copié.' : 'Le système refuse la copie.';
    });
    // Partager : un menu sous le bouton, refermé par un clic ailleurs.
    const fermerLeMenu = () => {
      if ($('menuPartage')) $('menuPartage').hidden = true;
      if ($('partager')) $('partager').setAttribute('aria-expanded', 'false');
      document.removeEventListener('mousedown', horsDuMenu, true);
    };
    function horsDuMenu(e) {
      if (!e.target.closest('#menuPartage, #partager')) fermerLeMenu();
    }
    $('partager').addEventListener('click', () => {
      if (!$('menuPartage').hidden) {
        fermerLeMenu();
        return;
      }
      $('menuPartage').hidden = false;
      $('partager').setAttribute('aria-expanded', 'true');
      document.addEventListener('mousedown', horsDuMenu, true);
    });
    if ($('copierLien')) {
      $('copierLien').addEventListener('click', async () => {
        fermerLeMenu();
        const ok = await discord.copier(lienDeLaRotation());
        $('copieDiscord').className = `copie ${ok ? 'ok' : 'refus'}`;
        $('copieDiscord').textContent = ok ? 'Lien copié.' : 'Le système refuse la copie.';
      });
    }
    $('enregistrerPartage').addEventListener('click', async () => {
      fermerLeMenu();
      try {
        const chemin = await enregistrerLePartage(contenuDuPartage(etat, r, chargeDuResultat), nomDuFichier(etat));
        if (chemin === null) return;
        $('copieDiscord').className = 'copie ok';
        $('copieDiscord').textContent = `Fichier enregistré : ${chemin.split(/[\\/]/).pop()}`;
      } catch (e) {
        $('copieDiscord').className = 'copie refus';
        $('copieDiscord').textContent = e.message;
      }
    });
  }

  /// Le build tel que la copie Discord et le compagnon le nomment.
  function nomDuBuild() {
    const b = etat.resolu || {};
    return `${b.class_name || 'Build'} ${b.level || ''}`.trim() + (etat.nomDuBuild ? ` · ${etat.nomDuBuild}` : '');
  }

  function dessinerDetail() {
    const r = resultat;
    // Une phrase par alerte, la liste des sorts dedans.
    const enListe = (liste) => (liste.length > 1
      ? `${liste.slice(0, -1).map(echapper).join(', ')} et ${echapper(liste[liste.length - 1])}`
      : echapper(liste[0]));
    const alerte = (liste, phrase) => (liste && liste.length
      ? `<p class="bandeau souci" style="margin-top:.6rem">${phrase(enListe(liste), liste.length > 1)}</p>` : '');

    // Un tour par ligne, qui s'ouvre d'un clic : fermé, ses dégâts, ses PA et ses
    // sorts ; ouvert, une ligne par lancer, et une pour les dégâts de début de tour
    // (poisons, effets à retardement). Ce qui est ouvert le reste d'un calcul à
    // l'autre.
    const icone = (x) => imageDe(x) || '<span class="sans-icone"></span>';
    etat.toursOuverts = etat.toursOuverts || {};
    $('detailRotation').innerHTML = `
      <div class="carte selectionnable${classeRepli(etat, 'rotation')}">
        ${titreRepliable(etat, 'rotation', 'Rotation')}
        <p class="aide">Cliquez un lancer pour le comparer aux autres sorts.</p>
        <div class="frises">${r.turns.map((t, i) => {
          // Le décompte que suit un joueur : ce que coûte chaque lancer, et ce
          // qui reste après. Un PA rendu (Bombance, Intrépide) se lit dans le
          // reste qui ne baisse pas.
          const depense = t.casts.reduce((somme, c) => somme + (c.ap || 0), 0);
          const budget = depense + Math.max(0, t.ap_left || 0);
          const ouverture = (t.opening_sources || []).filter((o) => o.damage > 0);
          const debut = ouverture.reduce((somme, o) => somme + o.damage, 0);
          return `
          <details class="palier" data-pli="${i}" ${etat.toursOuverts[i] ? 'open' : ''}>
            <summary><b>Tour ${t.turn != null ? t.turn : i + 1}</b>
              <span>${nb(t.damage)} dégâts${debut > 0 ? ` <i>dont ${nb(debut)} en début de tour</i>` : ''}</span>
              <span>${depense}/${budget} PA</span>
              ${t.ap_left > 0 ? `<span class="alerte">${t.ap_left} PA perdus</span>` : ''}
              <span class="sorts-du-tour">${t.casts.map((c) => echapper(c.name)).join(', ')}</span>
            </summary>
            <ol class="lancers">${ouverture.map((o) => `
              <li class="ouverture">
                ${icone(o)}
                <span class="nom">${echapper(o.name)} <span class="moment">${o.detail ? `${echapper(o.detail)}, ` : ''}début du tour</span></span>
                <span class="degats">${nb(o.damage)}</span>
                <span class="pa"></span><span class="reste"></span>
              </li>`).join('')}${t.casts.map((c, k) => `
              <li class="lancer" data-sort="${echapper(c.id)}" data-tour="${i}" data-lancer="${k}" tabindex="0" role="button">
                ${icone(c)}
                <span class="nom">${echapper(c.name)}${(c.indications || [])
                  .map((t) => `<span class="indication">${echapper(t)}</span>`).join('')}${((r.effets || {})[c.id] || [])
                  .map((p) => `<span class="effet-ligne">${echapper(p)}</span>`).join('')}</span>
                <span class="degats">${nb(c.damage)}</span>
                <span class="pa">−${c.ap} PA</span>
                <span class="reste">Reste ${c.ap_left}</span>
              </li>`).join('')}</ol>
            <div class="conseil" data-conseil="${i}" hidden></div>
          </details>${i === 0 ? hypotheses('tour1').map((m) => `<p class="note-tour">${echapper(majuscule(m))}.</p>`).join('') : ''}`;
        }).join('')}</div>

        ${alerte((r.ecartes || []).map((e) => e.sort),
          (sorts) => `Sorts jamais rentables ici : ${sorts}.`)}
        ${alerte(r.hors_position,
          (sorts, plusieurs) => `Écarté${plusieurs ? 's' : ''} par votre position : ${sorts}.`)}
        ${alerte(r.zones_sans_plafond,
          (sorts) => `Dégâts surestimés sur plusieurs ennemis pour ${sorts}.`)}
        ${r.targets > 1 && reglages.etalement === 0 ? alerte(r.zones_degressives,
          (sorts, plusieurs) => `Avec 0 case entre les ennemis touchés, ${sorts} ${
            plusieurs ? 'frappent' : 'frappe'} chaque ennemi à pleine puissance.`)
          : ''}
      </div>

      <div class="carte selectionnable${classeRepli(etat, 'repartition')}" style="margin-top:1rem">
        ${titreRepliable(etat, 'repartition', 'Répartition des dégâts par source')}
        <table class="mini">
          <thead><tr><th></th><th>Source</th><th>Dégâts</th><th>Part</th></tr></thead>
          <tbody>${(r.repartition || []).map((p) => `
          <tr><td style="width:1.6rem">${imageDe(p, 20)}</td>
            <td>${echapper(p.name)}</td>
            <td style="width:5rem">${nb(p.damage)}</td>
            <td style="width:4rem">${(p.share * 100).toFixed(1).replace('.', ',')} %</td></tr>`).join('')}
        </tbody></table>
      </div>
`;

    $('detailRotation').querySelectorAll('details.palier').forEach((d) => {
      d.addEventListener('toggle', () => { etat.toursOuverts[d.dataset.pli] = d.open; });
    });
    $('detailRotation').querySelectorAll('.lancer[data-tour]').forEach((el) => {
      const ouvrir = () => conseiller(Number(el.dataset.tour), Number(el.dataset.lancer));
      el.addEventListener('click', ouvrir);
      el.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          ouvrir();
        }
      });
    });

    $('graphiques').innerHTML = `
      <div class="bande-graphiques">
        <div class="carte${classeRepli(etat, 'par-tour')}">
          ${titreRepliable(etat, 'par-tour', 'Rythme')}
          ${rythme(r.turns, r.steady, r.targets)}
        </div>
        <div class="carte${classeRepli(etat, 'par-pa')}">
          ${titreRepliable(etat, 'par-pa', 'Dégâts par PA')}
          ${parPA(r.turns, r.repartition)}
        </div>
        <div class="carte${classeRepli(etat, 'ennemis')}">
          ${titreRepliable(etat, 'ennemis', 'Dégâts selon le nombre d\'ennemis')}
          <div id="blocCourbe"></div>
        </div>
        <div class="carte${classeRepli(etat, 'valeurs')}">
          ${titreRepliable(etat, 'valeurs', 'Ce que vaut chaque caractéristique')}
          <div id="blocValeurs"></div>
        </div>
      </div>`;
    dessinerCourbe();
    dessinerValeurs();
  }

  /// Les éléments d'une source de la répartition : `['meilleur']` pour un sort
  /// qui frappe dans le meilleur élément du build, rien pour ce qui n'est pas un
  /// sort du deck.
  const elementsDe = (id) => {
    const s = sorts.find((x) => x.id === id);
    if (!s) return [];
    return s.orientation && s.orientation.meilleur ? ['meilleur'] : (s.elements || []);
  };

  /// Ce que vaut chaque caractéristique : la rotation affichée recalculée cran
  /// par cran. Autant de résolutions complètes que de crans : elle se demande,
  /// et le nombre de calculs est dit avant.
  function dessinerValeurs() {
    const bloc = $('blocValeurs');
    if (!bloc || !resultat || !chargeDuResultat) return;
    if (valeurs) {
      bloc.innerHTML = dessinerLesValeurs(valeurs, critiqueDuResultat);
      return;
    }
    const n = aMesurer(resultat, chargeDuResultat, elementsDe, orientationDu(etat.resolu && etat.resolu.elements)).length;
    bloc.innerHTML = `
      <button type="button" class="action discret" id="mesurerValeurs">Mesurer</button>
      <p class="aide" id="etatValeurs">${n} calculs de la même rotation, un cran de plus à chaque fois.</p>`;
    $('mesurerValeurs').addEventListener('click', mesurerValeurs);
  }

  let arretDesValeurs = false;
  async function mesurerValeurs() {
    const demande = chargeDuResultat;
    const base = resultat.total;
    const liste = aMesurer(resultat, demande, elementsDe, orientationDu(etat.resolu && etat.resolu.elements));
    const bloc = $('blocValeurs');
    arretDesValeurs = false;
    $('calculer').disabled = true;
    bloc.innerHTML = `
      <div class="barre"><span id="jaugeValeurs"></span></div>
      <p class="aide" id="etatValeurs"></p>
      <button type="button" class="action discret petit" id="arreterValeurs">Annuler</button>`;
    $('arreterValeurs').addEventListener('click', () => {
      arretDesValeurs = true;
      appeler('annuler').catch(() => {});
    });
    const totaux = {};
    try {
      for (const [k, stat] of liste.entries()) {
        if (arretDesValeurs) throw new Error('calcul annulé');
        const { pas, nom } = CRANS[stat];
        $('etatValeurs').textContent = `Mesure ${k + 1} sur ${liste.length} : +${pas} ${nom}`;
        $('jaugeValeurs').style.width = `${Math.round((k / liste.length) * 100)}%`;
        const d = await appeler('rotation', { ...demande, ajout: { stat, montant: pas } });
        totaux[stat] = d.rotation.total;
      }
      // Un nouveau résultat arrivé entre-temps rend la mesure caduque.
      if (demande !== chargeDuResultat) return;
      valeurs = lireLesValeurs(base, totaux);
      dessinerValeurs();
    } catch (e) {
      if (demande !== chargeDuResultat) return;
      dessinerValeurs();
      $('etatValeurs').innerHTML = e.message === 'calcul annulé'
        ? 'Mesure annulée.' : `<span class="alerte">${echapper(e.message)}</span>`;
    } finally {
      $('calculer').disabled = false;
    }
  }

  /// Ce que chaque sort du deck vaudrait à la place d'un lancer, à cet instant :
  /// la rotation affichée rejouée jusqu'à lui, puis chaque sort à sa place. Un
  /// second clic sur le même lancer referme la liste.
  async function conseiller(tour, lancer) {
    const zone = $('detailRotation');
    const cle = `${tour}:${lancer}`;
    zone.querySelectorAll('[data-conseil]').forEach((h) => { h.hidden = true; });
    zone.querySelectorAll('.lancer.ouvert').forEach((x) => x.classList.remove('ouvert'));
    if (conseilOuvert === cle || !chargeDuResultat) {
      conseilOuvert = null;
      return;
    }
    conseilOuvert = cle;
    const lancerEl = zone.querySelector(`.lancer[data-tour="${tour}"][data-lancer="${lancer}"]`);
    if (lancerEl) lancerEl.classList.add('ouvert');
    const hote = zone.querySelector(`[data-conseil="${tour}"]`);
    hote.hidden = false;
    hote.innerHTML = '<p class="aide">Calcul…</p>';
    const tours = resultat.turns.map((t) => t.casts.map((c) => c.name));
    try {
      const d = await appeler('conseil', { requete: chargeDuResultat, tours, tour, lancer });
      if (conseilOuvert !== cle) return;
      hote.innerHTML = `<p class="aide">Autres sorts à la place ${/^[aeiouyàâäéèêëîïôöûüh]/i.test(d.joue) ? 'd\'' : 'de '}${echapper(d.joue)}</p>
        <table class="mini"><tbody>${d.remplacants.map((r) => `
          <tr${r.name === d.joue ? ' class="joue"' : ''}>
            <td style="width:1.6rem">${imageDe(r, 20)}</td>
            <td>${echapper(r.name)}${r.name === d.joue ? ' <span class="aide">(joué)</span>' : ''}</td>
            <td style="width:5rem">${nb(r.damage)}</td>
            <td style="width:3.5rem">${r.ap} PA</td>
          </tr>`).join('')}</tbody></table>
        <p class="aide">Dégâts du lancer seul, sans son effet sur les sorts suivants.</p>`;
    } catch (e) {
      if (conseilOuvert === cle) hote.innerHTML = `<p class="alerte">${echapper(e.message)}</p>`;
    }
  }

  /// Cette courbe coûte cher, chaque point étant une résolution complète, plus
  /// chère à plusieurs ennemis : elle se demande, et l'attente est dite avant.
  function dessinerCourbe() {
    const bloc = $('blocCourbe');
    if (!bloc) return;
    if (balayage) {
      bloc.innerHTML = selonCibles(balayage, resultat.targets)
        || '<p class="aide">Pas assez de points pour tracer la courbe.</p>';
      return;
    }
    const jusqua = borneBalayage();
    bloc.innerHTML = `
      <button type="button" class="action discret" id="tracerCourbe">
        Tracer de 1 à ${jusqua} ennemis</button>
      <p class="aide" id="etatCourbe"></p>`;
    $('tracerCourbe').addEventListener('click', tracerCourbe);
  }

  async function tracerCourbe() {
    const bouton = $('tracerCourbe');
    bouton.disabled = true;
    const depart = performance.now();
    const chrono = setInterval(() => {
      $('etatCourbe').textContent = `Calcul, ${((performance.now() - depart) / 1000).toFixed(1)} s…`;
    }, 100);
    try {
      // Le moteur résout avant de balayer : ce second appel refait la rotation
      // affichée, un point de plus que ceux tracés.
      const d = await appeler('rotation', charge({ targets_sweep: borneBalayage() }));
      balayage = d.rotation.targets_sweep || [];
      dessinerCourbe();
    } catch (e) {
      $('etatCourbe').innerHTML = `<span class="alerte">${echapper(e.message)}</span>`;
      bouton.disabled = false;
    } finally {
      clearInterval(chrono);
    }
  }

  /// Les invocations du build, un encart chacune : celles de sa classe, et les
  /// communes que le deck coche. Ni les % de dommages ni les dommages critiques
  /// de l'invocateur ne s'y appliquent. La réponse se garde : cocher une commune
  /// la fait apparaître sans redemander.
  let invocationsDuBuild = null;
  async function remplirInvocations() {
    if (!etat.build) return;
    try {
      invocationsDuBuild = (await appeler('invocations', etat.build)).invocations || [];
    } catch (e) {
      return;
    }
    dessinerInvocations();
  }

  function dessinerInvocations() {
    const hote = $('carteInvocations');
    if (!invocationsDuBuild) return;
    const idDuSort = new Map(sorts.map((s) => [s.dofusdb_id, s.id]));
    const propres = invocationsDuBuild.filter((i) => !i.commune);
    const communes = invocationsDuBuild.filter((i) => i.commune && reglages.coches.has(idDuSort.get(i.sort)));
    // La case des invocations de la classe ne sert qu'à une classe qui en a.
    $('caseInvocationsDeClasse').hidden = !propres.length;
    if (!propres.length && !communes.length) {
      hote.hidden = true;
      return;
    }
    const valeur = (f) => {
      if (!f) return '–';
      return f[0] === f[1] ? nb(f[0]) : `${nb(f[0])} à ${nb(f[1])}`;
    };
    // Ce que la rotation lui fait jouer à chaque tour.
    const jeu = (i) => {
      if (!i.jeu) return '';
      const morceaux = [`${i.jeu} par tour (${i.pa_depenses} PA sur ${i.pa})`];
      if (i.vie) morceaux.push(`${i.vie} tours durant, une à la fois`);
      if (i.autre) morceaux.push(`ou ${i.autre.invocation} à ${i.autre.chance} %`);
      return `<p class="jeu">${echapper(morceaux.join(', '))}.</p>`;
    };
    const encart = (i) => `
      <section class="encart"><h4>${echapper(i.invocation)} <span class="rang">· rang ${i.rang}</span></h4>
        <div>
          <p class="sort-source">${echapper(i.nom)}${i.facteur !== 100 ? `, ${i.facteur} % de vos caractéristiques` : ''}</p>
          <table class="mini">
            <thead><tr><th>Attaque</th><th>Normal</th><th>Critique</th><th>PA</th></tr></thead>
            <tbody>${i.attaques.map((a) => `<tr>
              <td style="color:${COULEUR_ELEMENT[majuscule(a.element)]}">${echapper(a.nom || i.invocation)}${a.vol ? ' (vol)' : ''}
                <span class="cibles">${echapper(a.cibles)}</span></td>
              <td>${valeur(a.normal)}</td><td>${valeur(a.critique)}</td><td>${a.pa ?? '?'}</td>
            </tr>`).join('')}</tbody>
          </table>
          ${jeu(i)}
        </div>
      </section>`;
    hote.classList.toggle('repliee', Boolean(etat.replies && etat.replies.invocations));
    hote.innerHTML = `${titreRepliable(etat, 'invocations', 'Invocations')}
      ${hypotheses('invocations').length ? `<ul class="aide remarques">${hypotheses('invocations')
    .map((m) => `<li>${echapper(majuscule(m))}.</li>`).join('')}</ul>` : ''}
      <p class="aide">Calculés avec vos caractéristiques, sans vos % de dommages ni vos
        dommages critiques.${(propres.length && reglages.invocationsDeClasse === false)
    || (communes.length && !reglages.invocationsCommunes)
    ? ' Cochez-les dans le panneau, sous « Invocations », pour qu\'elles jouent dans la rotation.' : ''}</p>
      ${propres.length ? `<div class="grille-encarts">${propres.map(encart).join('')}</div>` : ''}
      ${communes.length ? `<p class="groupe-encarts">Communes à toutes les classes, cochées</p>
        <div class="grille-encarts">${communes.map(encart).join('')}</div>` : ''}`;
    hote.hidden = false;
  }

  // Les barres de titre replient leur encart, une fois branchées pour toute
  // la page.
  brancherRepli(cible, etat);
  majModifies();
  dessinerSorts();
  remplirInvocations();
}
