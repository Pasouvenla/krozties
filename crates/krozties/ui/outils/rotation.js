// Rotation : quels sorts lancer, dans quel ordre.

import { COULEUR_ELEMENT, echapper, FR_ELEMENT, majuscule, nb, pluriel } from '../commun/format.js';
import { parTour, selonCibles } from '../commun/graphique.js';
import { brancherRepli, classeRepli, titreRepliable } from '../commun/repli.js';

/// Le moteur n'accepte pas plus de trente-deux sorts (masques en `u32`) : le
/// bouton se ferme avant.
const MAX_SORTS = 32;

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
  /// Ce que les objets de classe du build changent aux sorts : leur coût en PA
  /// d'abord, que le deck affiche et classe.
  const modifs = (etat.resolu && etat.resolu.sorts_modifies) || {};
  const sorts = classe
    ? [...classe.spells.map((s) => (modifs[s.id] ? { ...s, ...modifs[s.id] } : s)),
      ...(arme ? [arme] : []), ...sortsDObjets]
    : [];
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
  /// `resolu.dominant` rend « Eau », les sorts portent « Water » : ne pas les
  /// comparer directement.
  ///
  /// Les sorts de l'orientation : ceux qui frappent dans un élément dominant du
  /// build ou dans son meilleur élément, pièges et invocations compris, puis ceux qui
  /// agissent sans frapper. Les sorts de l'élément d'abord : dans une paire, la
  /// variante gardée est la première venue.
  const orientation = () => {
    const dominants = (etat.resolu && etat.resolu.dominant) || [];
    const o = (s) => s.orientation || { elements: s.elements || [], meilleur: false, agit: true };
    const vaut = (s) => o(s).meilleur || o(s).elements.some((e) => dominants.includes(FR_ELEMENT[e] || e));
    const neutre = (s) => !vaut(s) && !o(s).elements.length && o(s).agit;
    // Deux sorts de l'élément dans une paire : celui qui rend le plus par PA
    // sur ce build passe devant, Poisse plutôt que Cruauté sur un Sram Eau.
    const tables = (etat.resolu && etat.resolu.damage_tables) || {};
    const parPa = (s) => {
      const t = tables[s.id] && tables[s.id][0];
      return t && s.ap ? (t.normal[0] + t.normal[1]) / 2 / s.ap : 0;
    };
    return [...sorts.filter(vaut).sort((a, b) => parPa(b) - parPa(a)), ...sorts.filter(neutre)];
  };

  const reglages = etat.rotation || (etat.rotation = {
    coches: null,
    horizon: 7,
    pmDepenses: 0,
    /// La Maîtrise d'arme, en Puissance : la normale par défaut.
    maitrise: 300,
    /// Les poussées butent contre un obstacle : leurs dommages comptent.
    pousseesBloquees: false,
    cibles: 1,
    etalement: 0,
    /// Chercher sur tous les sorts à la fois, au lieu des deux passes : plus
    /// long, rarement meilleur.
    exhaustif: false,
    /// Ce que le joueur déclare de sa situation, par identifiant de réglage.
    etats: {},
    /// Où il se tient : `libre`, `distance` ou `melee`.
    position: 'libre',
  });
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
  /// Un défaut ne peut pas être injouable : on garde la première variante de
  /// chaque paire, et le joueur change celle qu'il veut.
  const uneParPaire = (liste) => {
    const vus = new Set();
    return liste.filter((s) => {
      const clef = s.variant_group != null ? `g${s.variant_group}` : `s${s.id}`;
      if (vus.has(clef)) return false;
      vus.add(clef);
      return true;
    });
  };

  if (!reglages.coches) {
    // Par défaut, ce que le build sait vraiment faire : les sorts de son
    // orientation, une seule variante par paire, bornés au plafond du moteur.
    reglages.coches = new Set(uneParPaire(orientation()).slice(0, MAX_SORTS).map((s) => s.id));
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
  let enCours = false;
  let perime = false;

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
          <details class="plus-loin" id="plusLoin" ${etat.plusLoinOuvert ? 'open' : ''}>
            <summary>Pour aller plus loin <span class="modifies" id="reglagesModifies"></span></summary>
            <div class="reglages">
              <div><label for="pmDepenses">PM dépensés</label>
                <input type="number" id="pmDepenses" min="0" max="12" value="${reglages.pmDepenses}"></div>
              <div><label for="etalement">Écart entre ennemis</label>
                <input type="number" id="etalement" min="0" max="4" value="${reglages.etalement}"></div>
              ${arme ? `<div><label for="maitrise">Maîtrise d'arme</label>
                <select id="maitrise">
                  ${[[0, 'Aucune'], [300, 'Normale (300)'], [360, 'Critique (360)']]
                    .map(([v, t]) => `<option value="${v}"${(reglages.maitrise ?? 300) === v ? ' selected' : ''}>${t}</option>`).join('')}
                </select></div>` : ''}
            </div>
            <h4 class="groupe">Votre combat</h4>
            ${declares.filter((d) => Number(d.max) > 1).length ? `<div class="reglages">${
              declares.filter((d) => Number(d.max) > 1).map((d) => `
              <div><label for="etat-${echapper(d.id)}">${echapper(d.label)}</label>
                <input type="number" id="etat-${echapper(d.id)}" data-etat="${echapper(d.id)}"
                  min="0" max="${Number(d.max)}" value="${reglages.etats[d.id]}"></div>`).join('')}</div>` : ''}
            <label class="bascule" for="pousseesBloquees">
              <input type="checkbox" id="pousseesBloquees" ${reglages.pousseesBloquees ? 'checked' : ''}>
              <span>Vos poussées butent contre un obstacle</span>
            </label>
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
    const carte = (s) => `
      <label class="sort${reglages.coches.has(s.id) ? ' coche' : ''}" data-sort="${echapper(s.id)}">
        <input type="checkbox" value="${echapper(s.id)}" ${reglages.coches.has(s.id) ? 'checked' : ''}>
        ${imageDe(s) || '<span class="vide"></span>'}
        <span>${echapper(s.name)}</span>
        <span class="cout">${s.ap} PA</span>
      </label>`;

    $('listeSorts').innerHTML = paires.map((g) => {
      const conflit = g.length > 1 && g.every((s) => reglages.coches.has(s.id));
      return `<div class="paire${conflit ? ' conflit' : ''}"
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
        majCompteur();
        marquerPerime();
      });
    });
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
      Boolean(reglages.pousseesBloquees),
      Boolean(reglages.exhaustif),
      estRoublard && Boolean(reglages.mur),
      ...declares.map((d) => Number(reglages.etats[d.id]) !== (Number(d.defaut) || 0)),
      ...bonusDofus.map((b) => cochee(b) !== (b.coche !== false)),
    ].filter(Boolean).length;
    $('reglagesModifies').textContent = n ? `${n} ${pluriel(n, 'modifié')}` : '';
  }

  function marquerPerime() {
    majModifies();
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
    reglages.coches = new Set(uneParPaire(orientation()).slice(0, MAX_SORTS).map((s) => s.id));
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
    targets: reglages.cibles,
    etalement: reglages.etalement,
    exhaustif: reglages.exhaustif,
    etats_declares: Object.fromEntries(declares.map((d) => [d.id, reglages.etats[d.id]])),
    bonus_ecartes: ecartes(),
    bonus_inclus: inclus(),
    ...(estRoublard ? { mur_de_bombes: mur() } : {}),
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
      perime = false;
      $('panneau').classList.remove('perime');
      $('calculer').textContent = 'Recalculer';
      $('etatCalcul').textContent = '';
      dessinerResume();
      dessinerDetail();
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

  function dessinerResume() {
    const r = resultat;
    const st = r.steady || {};
    // Dès le seuil du critique sûr au build, le moteur compte en critique tout coup
    // qui peut critiquer : la règle se dit sous le total qu'elle fait monter.
    const certain = etat.classes && etat.classes.limites && etat.classes.limites.critique_certain;
    const toutCritique = certain != null && critiqueDuResultat != null && critiqueDuResultat >= certain;
    // « Résultat » : « Rotation » est le nom de l'encart des tours, à gauche.
    $('resumeRotation').innerHTML = `
      <h3 style="margin-top:.9rem">Résultat</h3>
      <div class="chiffre">${nb(r.total)}</div>
      <div class="sous-chiffre">dégâts sur ${r.turns.length} ${pluriel(r.turns.length, 'tour')} · ${
        nb(r.per_turn)} par tour</div>
      ${toutCritique ? `<p class="aide">Dès ${certain} de critique, tous les sorts sont comptés en coup critique.</p>` : ''}
      <dl>
        ${st.cycle_length ? `<dt title="La rotation se répète tous les ${st.cycle_length} ${
          pluriel(st.cycle_length, 'tour')}">Sur la durée</dt><dd>${nb(st.per_turn)} par tour</dd>` : ''}
        ${r.ap_wasted > 0 ? `<dt>PA perdus</dt><dd class="alerte">${r.ap_wasted}</dd>` : ''}
      </dl>
      ${st.too_large ? '<p class="aide">Moyenne sur la durée non calculée avec autant de sorts.</p>' : ''}`;
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
                <span class="nom">${echapper(c.name)}</span>
                <span class="degats">${nb(c.damage)}</span>
                <span class="pa">−${c.ap} PA</span>
                <span class="reste">Reste ${c.ap_left}</span>
              </li>`).join('')}</ol>
            <div class="conseil" data-conseil="${i}" hidden></div>
          </details>`;
        }).join('')}</div>

        ${alerte((r.ecartes || []).map((e) => e.sort),
          (sorts) => `Sorts jamais rentables ici : ${sorts}.`)}
        ${alerte(r.hors_position,
          (sorts, plusieurs) => `Écarté${plusieurs ? 's' : ''} par votre position : ${sorts}.`)}
        ${alerte(r.zones_sans_plafond,
          (sorts) => `Dégâts surestimés sur plusieurs ennemis pour ${sorts}.`)}
        ${r.targets > 1 && reglages.etalement === 0 ? alerte(r.zones_degressives,
          (sorts, plusieurs) => `Avec un écart à 0, ${sorts} ${plusieurs ? 'frappent' : 'frappe'} chaque ennemi à pleine puissance.`)
          : ''}
      </div>

      <div class="carte selectionnable${classeRepli(etat, 'repartition')}" style="margin-top:1rem">
        ${titreRepliable(etat, 'repartition', 'Répartition des dégâts par source')}
        <table class="mini"><tbody>${(r.repartition || []).map((p) => `
          <tr><td style="width:1.6rem">${imageDe(p, 20)}</td>
            <td>${echapper(p.name)}</td>
            <td style="width:5rem">${nb(p.damage)}</td>
            <td style="width:4rem">${(p.share * 100).toFixed(1)} %</td></tr>`).join('')}
        </tbody></table>
      </div>
      ${(r.remarques || []).length ? `
      <div class="carte selectionnable${classeRepli(etat, 'remarques')}" style="margin-top:1rem">
        ${titreRepliable(etat, 'remarques', 'Remarques du calcul')}
        <ul class="aide remarques">${r.remarques.map((m) => `<li>${echapper(majuscule(m))}</li>`).join('')}</ul>
      </div>` : ''}`;

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
          ${titreRepliable(etat, 'par-tour', 'Dégâts par tour')}
          ${parTour(r.steady, r.turns, r.targets)}
        </div>
        <div class="carte${classeRepli(etat, 'ennemis')}">
          ${titreRepliable(etat, 'ennemis', 'Dégâts selon le nombre d\'ennemis')}
          <div id="blocCourbe"></div>
        </div>
      </div>`;
    dessinerCourbe();
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

  /// Les invocations du build, attaque par attaque : ni les % de dommages ni les
  /// dommages critiques de l'invocateur ne s'y appliquent.
  async function remplirInvocations() {
    const hote = $('carteInvocations');
    if (!etat.build) return;
    let reponse;
    try {
      reponse = await appeler('invocations', etat.build);
    } catch (e) {
      return;
    }
    const liste = reponse.invocations || [];
    if (!liste.length) return;
    const fourchette = (f) => (f ? `${nb(f[0])} à ${nb(f[1])}` : '–');
    const bloc = (i) => `
      <h4>${echapper(i.invocation)}${i.invocation !== i.nom ? ` <span class="aide">(${echapper(i.nom)})</span>` : ''}
        <span class="aide">rang ${i.rang}${i.facteur !== 100 ? `, ${i.facteur} % de vos caractéristiques` : ''}</span></h4>
      <table class="mini">
        <thead><tr><th>Attaque</th><th>Normal</th><th>Critique</th><th>PA</th></tr></thead>
        <tbody>${i.attaques.map((a) => `<tr>
          <td style="color:${COULEUR_ELEMENT[majuscule(a.element)]}">${echapper(a.nom || i.invocation)}${a.vol ? ' (vol)' : ''}
            <span class="aide">${echapper(a.cibles)}</span></td>
          <td>${fourchette(a.normal)}</td><td>${fourchette(a.critique)}</td><td>${a.pa ?? '?'}</td>
        </tr>`).join('')}</tbody>
      </table>`;
    const propres = liste.filter((i) => !i.commune);
    const communes = liste.filter((i) => i.commune);
    hote.classList.toggle('repliee', Boolean(etat.replies && etat.replies.invocations));
    hote.innerHTML = `${titreRepliable(etat, 'invocations', 'Invocations')}
      <p class="aide">Calculés avec vos caractéristiques, sans vos % de dommages ni vos
        dommages critiques.</p>
      ${propres.map(bloc).join('')}
      ${communes.length ? `<h4>Communes à toutes les classes</h4>${communes.map(bloc).join('')}` : ''}`;
    hote.hidden = false;
  }

  // Les barres de titre replient leur encart, une fois branchées pour toute
  // la page.
  brancherRepli(cible, etat);
  majModifies();
  dessinerSorts();
  remplirInvocations();
}
