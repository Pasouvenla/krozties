// Zones de sorts : où un sort frappe, et ce que chaque ennemi prend. Un seul
// sort part à la fois ; la géométrie reste au serveur, la page dessine ce qu'on
// lui répond.

import { contourZone, dessiner, distance, losange, memeCase, versEcran } from '../commun/damier.js';
import { placer } from '../commun/etiquettes.js';
import { COULEUR_ELEMENT, echapper, majuscule, nb, pluriel } from '../commun/format.js';
import { ficheDeSort } from '../commun/infobulle.js';
import {
  HAUTEUR_POUTCH, fantome, icone, interdit, jetonLettre, poutchs, retrait,
} from '../commun/pions.js';
import {
  bilanDuRecalage, chargerCartes, choisir, garderLeSol, plateauActif, pourLeServeur,
  recaler, selecteur,
} from '../commun/plateaux.js';

/// Le plafond d'ennemis vient du moteur, par le catalogue ; le repli ne sert que
/// si le catalogue manque.
const plafondEnnemis = (etat) => (etat.classes && etat.classes.limites
  && etat.classes.limites.ennemis) || 1;

export async function rendre(cible, etat, { appeler }) {
  const MAX_ENNEMIS = plafondEnnemis(etat);
  const placement = etat.placement || (etat.placement = {
    lanceur: [-4, 0],
    visee: [0, 0],
    ennemis: [[0, 0]],
  });
  let sortChoisi = null;
  let reponse = null;
  let refus = '';
  await chargerCartes(etat, appeler);
  let plateau = plateauActif(etat);

  /// Ce qui n'est plus sur le sol du plateau en revient : le lanceur et la
  /// visée sur la case de sol la plus proche, les ennemis retirés.
  function recalerPlacement() {
    let deplaces = 0;
    ['lanceur', 'visee'].forEach((cle) => {
      const ici = recaler(placement[cle], plateau);
      if (ici && !memeCase(ici, placement[cle])) { placement[cle] = ici; deplaces += 1; }
    });
    return bilanDuRecalage(deplaces, garderLeSol(placement.ennemis, plateau));
  }
  refus = recalerPlacement();

  /// La classe se choisit : l'écran s'ouvre sans équipement, sur celle de
  /// l'équipement chargé par défaut.
  const classes = (etat.classes && etat.classes.classes) || [];
  const classeDuBuild = etat.resolu && etat.resolu.class;
  let classeId = etat.zonesClasse
    || classeDuBuild
    || (classes[0] && classes[0].id);
  const sortsDe = (id) => ((classes.find((c) => c.id === id) || {}).spells || [])
    .filter((s) => (s.elements || []).length)
    .sort((a, b) => a.name.localeCompare(b.name, 'fr', { sensitivity: 'base' }));
  let sorts = sortsDe(classeId);

  /// L'équipement ne vaut que pour sa classe : hors de la classe chargée, le calcul
  /// part d'un niveau 200 sans équipement, et le tableau le dit. La case « Avec le
  /// build importé » se coche d'office dès qu'un build de la classe observée est
  /// chargé.
  const buildDeLaClasse = () => Boolean(etat.build) && classeId === classeDuBuild;
  const avecEquipement = () => buildDeLaClasse() && etat.zonesAvecBuild !== false;
  const buildPour = () => (avecEquipement() ? etat.build : { class: classeId, level: 200 });

  cible.innerHTML = `
    <section class="outil">
      <p class="sous">Choisissez un sort et placez vos ennemis.</p>

      <div class="vue-damier">
        <div>
          <div class="barre-outils">${selecteur('choixPlateauZone', etat)}</div>
          <div class="barre-outils">
            <label class="segment"><input type="radio" name="outilZone" value="ennemi" checked> Ennemi</label>
            <label class="segment"><input type="radio" name="outilZone" value="lanceur"> Lanceur</label>
            <label class="segment"><input type="radio" name="outilZone" value="visee"> Case visée</label>
            <button type="button" class="action discret" id="viderZone">Vider</button>
          </div>
          <div class="plateau" id="plateauZone"></div>
          <p class="aide" id="etatZone"></p>
          <details class="plus-loin">
            <summary>Pour aller plus loin</summary>
            <p class="aide" id="detailsZone"></p>
          </details>
        </div>

        <div class="carte">
          <h3>Sort</h3>
          <select id="classeZone" aria-label="Classe">
            ${classes.map((c) => `<option value="${c.id}" ${c.id === classeId ? 'selected' : ''}>${echapper(c.label)}</option>`).join('')}
          </select>
          <select id="sortChoisi" aria-label="Sort"></select>
          <label class="bascule" for="avecBuild">
            <input type="checkbox" id="avecBuild">
            <span>Avec le build importé</span>
          </label>
          <p class="aide palier-aide" id="aideBuild"></p>
          <div id="detailZone"></div>
          <div id="ficheZone" class="fiche-sort"></div>
        </div>
      </div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);
  function remplirSorts() {
    $('sortChoisi').innerHTML = sorts
      .map((s) => `<option value="${echapper(s.id)}">${echapper(s.name)}</option>`).join('');
    sortChoisi = sorts.length ? sorts[0].id : null;
    if (!sorts.length) {
      $('detailZone').innerHTML = '<p class="aide">Aucun sort de dégâts pour cette classe.</p>';
    }
  }
  remplirSorts();

  /// La case suit la classe observée : cochable avec un build de cette classe,
  /// grisée sinon, et la ligne d'aide dit quoi faire pour qu'elle le soit.
  function majCaseBuild() {
    const caseBuild = $('avecBuild');
    caseBuild.disabled = !buildDeLaClasse();
    caseBuild.checked = avecEquipement();
    $('aideBuild').textContent = !etat.build
      ? "Importez un build dans l'onglet Équipement."
      : !buildDeLaClasse()
        ? 'Choisissez la classe du build importé.'
        : '';
  }
  majCaseBuild();

  const outilActif = () =>
    (cible.querySelector('input[name=outilZone]:checked') || {}).value || 'ennemi';

  function cliquer(x, y) {
    const quoi = outilActif();
    if (quoi === 'lanceur') placement.lanceur = [x, y];
    else if (quoi === 'visee') placement.visee = [x, y];
    else {
      const i = placement.ennemis.findIndex((e) => memeCase(e, [x, y]));
      if (i >= 0) { placement.ennemis.splice(i, 1); refus = ''; }
      else if (placement.ennemis.length >= MAX_ENNEMIS) {
        // Refusé ET DIT : un clic qui ne fait rien sans un mot laisse croire
        // que le damier est cassé.
        refus = ` ⚠️ ${MAX_ENNEMIS} ennemis au plus : retirez-en un, en cliquant dessus, avant d'en poser un autre.`;
        peindre();
        return;
      } else { refus = ''; placement.ennemis.push([x, y]); }
    }
    peindre();
    interroger();
  }

  cible.querySelectorAll('input[name=outilZone]').forEach((radio) => {
    radio.addEventListener('change', () => { survoler(null, null); peindre(); });
  });
  $('choixPlateauZone').addEventListener('change', () => {
    choisir(etat, $('choixPlateauZone').value);
    plateau = plateauActif(etat);
    reponse = null;
    refus = recalerPlacement();
    peindre();
    interroger();
  });
  $('viderZone').addEventListener('click', () => {
    placement.ennemis = [];
    refus = '';
    peindre();
    interroger();
  });
  $('classeZone').addEventListener('change', () => {
    classeId = Number($('classeZone').value);
    etat.zonesClasse = classeId;
    sorts = sortsDe(classeId);
    reponse = null;
    remplirSorts();
    majCaseBuild();
    interroger();
  });
  $('avecBuild').addEventListener('change', () => {
    etat.zonesAvecBuild = $('avecBuild').checked;
    interroger();
  });
  $('sortChoisi').addEventListener('change', () => {
    sortChoisi = $('sortChoisi').value;
    interroger();
  });

  const sortDuCatalogue = () => sorts.find((s) => s.id === sortChoisi) || null;

  /// La fiche du sort observé : sa description et ses effets, chiffrés sur le
  /// build quand la case est cochée.
  function remplirFiche() {
    const sort = sortDuCatalogue();
    const r = etat.resolu;
    $('ficheZone').innerHTML = sort
      ? ficheDeSort(sort, {
        critique: avecEquipement() && r ? r.crit : undefined,
        tables: avecEquipement() && r ? r.damage_tables : undefined,
        modifs: avecEquipement() && r ? r.sorts_modifies : undefined,
        certain: etat.classes && etat.classes.limites && etat.classes.limites.critique_certain,
      })
      : '';
  }

  /// Pourquoi le sort ne part pas sur la case visée, en une phrase.
  function phraseDeRefus(refusVisee) {
    if (!refusVisee) return '';
    const { motif, distance: d, limite } = refusVisee;
    if (motif === 'trop_loin') {
      return ` La case visée est hors de portée : ${d} ${pluriel(d, 'case')}, ${limite} au plus.`;
    }
    if (motif === 'trop_pres') {
      return ` La case visée est trop près : ${d} ${pluriel(d, 'case')}, ${limite} au moins.`;
    }
    if (motif === 'hors_axe') {
      const c = (sortDuCatalogue() || {}).cast || {};
      const axe = c.in_line && c.in_diagonal ? 'en ligne ou en diagonale'
        : c.in_diagonal ? 'en diagonale' : 'en ligne';
      return ` Ce sort se lance ${axe}, et la case visée n'y est pas.`;
    }
    if (motif === 'case_vide') return ' Ce sort exige une cible sur la case visée.';
    if (motif === 'case_occupee') return ' Ce sort exige une case libre.';
    if (motif === 'hors_vue') return " La case visée n'est pas en ligne de vue : un mur ou un corps la cache.";
    if (motif === 'hors_sol') return " La case visée n'est pas du sol.";
    return '';
  }

  /// À 100 % de critique, c'est la fourchette critique qui tombe à chaque coup : la
  /// carte la montre.
  const critiqueSur = (z) => Boolean(z) && z.critique >= 100;
  const fourchetteAffichee = (z, t) => (critiqueSur(z) && t.critique ? t.critique : t.normal);

  /// Les coups d'un état qui frappe : un poison, un glyphe. Le premier qui
  /// atteint quelqu'un donne ses chiffres à la carte quand le sort ne frappe
  /// pas au lancer.
  const etatChiffre = (z) => (z && (z.etats || []).find((e) => e.touches.length)) || null;
  const coupDAffiche = (e, t) => (e.critique >= 100 && t.critique ? t.critique : t.normal);
  /// Combien de fois l'état frappe au début des tours, s'il y frappe.
  const coupsDe = (e) => {
    const d = e.declencheurs.find((x) => x.quand === 'debut_de_tour');
    return (d && d.coups) || 0;
  };
  const liste = (noms) => (noms.length > 1
    ? `${noms.slice(0, -1).join(', ')} ou ${noms[noms.length - 1]}` : noms[0] || '');

  /// Quand un état frappe, en clair. Le serveur donne la détente en données.
  function phraseDeclencheur(d) {
    const cible = d.porteur !== 'lanceur';
    let p;
    if (d.quand === 'debut_de_tour') {
      const de = cible ? ' de la cible' : '';
      p = d.coups == null ? `au début de chaque tour${de}`
        : d.coups === 1 ? `au début ${cible ? 'du prochain tour de la cible' : 'de votre prochain tour'}`
          : `au début de chacun ${cible ? 'des' : 'de vos'} ${d.coups} prochains tours${de}`;
    } else if (d.quand === 'sort') {
      p = `quand vous lancez ${echapper(liste(d.sorts))}`;
    } else if (d.quand === 'poussee') {
      p = cible ? 'quand la cible subit des dommages de poussée'
        : 'quand vous subissez des dommages de poussée';
    } else if (d.quand === 'perte') {
      const etat = d.etat ? `l'état ${echapper(d.etat)}` : 'son état';
      p = cible ? `quand la cible perd ${etat}` : `quand vous perdez ${etat}`;
    } else {
      p = 'à chaque déclenchement';
    }
    if (d.une_fois_par_tour) p += ', une fois par tour';
    if (d.si) p += `, si vous déclarez dans la rotation « ${echapper(d.si)} »`;
    else if (d.sous_condition) p += ', sous condition';
    return p;
  }

  /// Ce que le serveur a répondu pour le sort observé.
  const vu = () => (reponse && (reponse.sorts || []).find((s) => s.sort === sortChoisi)) || null;

  /// Ce qu'un clic poserait sous le curseur : une croix sur un ennemi déjà là,
  /// l'icône du sort sur la visée, cernée de rouge là d'où il ne part pas.
  function apercu(x, y) {
    const ici = [x, y];
    const quoi = outilActif();
    if (quoi === 'ennemi') {
      if (placement.ennemis.some((e) => memeCase(e, ici))) return retrait(ici);
      if (placement.ennemis.length >= MAX_ENNEMIS) return interdit(ici);
      return fantome(poutchs([{ case: ici, rang: placement.ennemis.length + 1 }]));
    }
    if (quoi === 'lanceur') return fantome(jetonLettre(ici, 'lanceur', 'V'));
    const z = vu();
    const portee = z && z.portee;
    const refusee = portee && portee.bornes && !(portee.cases || []).some((c) => memeCase(c, ici));
    const sort = sortDuCatalogue();
    return (refusee ? interdit(ici) : '') + fantome(icone(ici, sort && sort.dofusdb_id));
  }

  /// La zone qu'on obtiendrait en visant la case survolée, avant le clic : seul le
  /// calque du fantôme se redessine.
  function survoler(x, y) {
    const calque = $('plateauZone').querySelector('#fantomeZone');
    if (!calque) return;
    const z = vu();
    const cases = x != null && outilActif() === 'visee' && z && z.survol
      ? z.survol[`${x},${y}`] : null;
    if (!cases || !cases.length) { calque.innerHTML = ''; return; }
    const couleur = COULEUR_ELEMENT[z.element] || 'var(--accent)';
    calque.innerHTML = cases
      .map((c) => `<polygon class="zone-fond" fill="${couleur}" points="${losange(c[0], c[1])}"/>`).join('')
      + `<path class="zone-contour" stroke="${couleur}" d="${contourZone(cases)}"/>`;
  }

  function peindre() {
    const z = vu();
    const dansZone = new Set(((z && z.cases) || []).map((c) => `${c[0]},${c[1]}`));
    const portee = (z && z.portee) || null;
    const lancable = new Set(((portee && portee.cases) || []).map((c) => `${c[0]},${c[1]}`));
    const masquee = new Set(((portee && portee.masquees) || []).map((c) => `${c[0]},${c[1]}`));

    let calques = '';
    if (z && z.cases && z.cases.length) {
      const couleur = COULEUR_ELEMENT[z.element] || 'var(--accent)';
      calques += z.cases
        .map((c) => `<polygon class="zone-fond" fill="${couleur}" points="${losange(c[0], c[1])}"/>`).join('')
        + `<path class="zone-contour" stroke="${couleur}" d="${contourZone(z.cases)}"/>`;
    }
    // Le liseré de la case visée par-dessus la zone : c'est le repère du tir,
    // et un trait recouvert par la case voisine ne se voit plus.
    calques += `<polygon class="visee-trait" points="${losange(placement.visee[0], placement.visee[1])}"/>`;
    calques += '<g id="fantomeZone" class="fantome"></g>';

    let jetons = '';
    const [lx, ly] = versEcran(placement.lanceur[0], placement.lanceur[1]);
    jetons += `<text class="jeton lanceur" x="${lx}" y="${ly}">V</text>`;
    // Les ennemis en Poutch, leur numéro à leurs pieds : c'est lui que le
    // tableau des dégâts reprend.
    jetons += poutchs(placement.ennemis.map((e, i) => ({ case: e, rang: i + 1 })));

    // Un sort qui ne frappe pas au lancer montre les coups de l'état qu'il
    // pose, suivis de leur nombre : « 57 ×2 », deux coups de 57.
    const direct = Boolean(z && !z.refus && (z.touches || []).some((t) => t.normal));
    const parEtat = z && !z.refus && !direct ? etatChiffre(z) : null;
    const coups = parEtat ? coupsDe(parEtat) : 0;
    let masquees = 0;
    if (direct || parEtat) {
      const couleur = COULEUR_ELEMENT[(parEtat || z).element] || 'var(--encre)';
      const items = (parEtat ? parEtat.touches : z.touches)
        .map((t) => ({ t, f: parEtat ? coupDAffiche(parEtat, t) : fourchetteAffichee(z, t) }))
        .filter(({ f }) => f).map(({ t, f }) => ({
          case: t.case,
          // Un seul nombre sur la carte, le milieu de la fourchette ; la fourchette est
          // dans le tableau.
          texte: nb(Math.round((f[0] + f[1]) / 2)) + (coups > 1 ? ` ×${coups}` : ''),
          couleur,
        }));
      const pose = placer(items, [
        ...placement.ennemis.map((e) => [e[0], e[1], HAUTEUR_POUTCH]), placement.lanceur]);
      jetons += pose.svg;
      masquees = pose.masquees;
    }

    dessiner($('plateauZone'), {
      plateau,
      case: (x, y) => {
        const classes = [];
        if (dansZone.has(`${x},${y}`)) classes.push('zone');
        if (lancable.has(`${x},${y}`)) classes.push('portee');
        if (masquee.has(`${x},${y}`)) classes.push('portee-masquee');
        if (memeCase(placement.lanceur, [x, y])) classes.push('lanceur');
        if (placement.ennemis.some((e) => memeCase(e, [x, y]))) classes.push('ennemi');
        const d = distance([x, y], placement.visee);
        return {
          classes,
          titre: memeCase(placement.lanceur, [x, y]) ? 'Vous'
            : placement.ennemis.some((e) => memeCase(e, [x, y])) ? 'Ennemi'
            : d === 0 ? 'Case visée'
            : `${d} ${pluriel(d, 'case')} de la visée`,
        };
      },
      calques,
      jetons,
      surClic: cliquer,
      surSurvol: survoler,
      apercu,
    });

    // La légende en une ligne, le détail replié dans « Pour aller plus loin ».
    const couleurs = [];
    if (portee && portee.bornes) couleurs.push('En bleu la portée');
    if (portee && portee.bornes && masquee.size) couleurs.push('en gris les cases hors de vue');
    let texte = couleurs.length ? `${couleurs.join(', ')}.` : '';
    if (z) texte += phraseDeRefus(portee && portee.visee);
    // ⚠️ LE PIÈGE DU DAMIER : un sort mono-cible ne couvre QUE la case visée.
    if (placement.ennemis.length
        && !placement.ennemis.some((e) => memeCase(e, placement.visee))) {
      texte += ' Ce sort sans zone ne frappe que la case visée, où personne ne se tient.';
    }
    $('etatZone').textContent = (texte + refus).trim();

    const distances = placement.ennemis
      .map((e) => distance(e, placement.visee))
      .sort((a, b) => a - b);
    const libelle = (d) => (d === 0 ? 'sur la visée' : `${d} ${pluriel(d, 'case')}`);
    let details = placement.ennemis.length
      ? `${placement.ennemis.length} ${pluriel(placement.ennemis.length, 'ennemi')} : `
        + `${distances.map(libelle).join(', ')}.`
      : 'Aucun ennemi posé.';
    if (parEtat) {
      details += ` Les nombres sur la carte sont la moyenne d'un coup${
        parEtat.critique >= 100 ? ' critique' : ''} de l'état posé, au palier de base${
        coups > 1 ? ', suivie du nombre de coups' : ''}.`;
    } else if (direct) {
      details += critiqueSur(z)
        ? ' Les nombres sur la carte sont la moyenne du coup critique, au palier de base.'
        : ' Les nombres sur la carte sont la moyenne de la fourchette, au palier de base.';
    }
    // La règle du critique sûr, dite comme dans la Rotation : la fiche montre le
    // vrai taux, le tableau et la carte comptent le critique.
    const seuil = etat.classes && etat.classes.limites && etat.classes.limites.critique_certain;
    if (z && seuil != null && reponse && reponse.critique_du_build >= seuil) {
      details += ` Dès ${seuil} de critique, tous les sorts sont comptés en coup critique.`;
    }
    if ((direct || parEtat) && masquees) {
      details += ` ${masquees} ${pluriel(masquees, 'étiquette')} `
        + `${masquees > 1 ? 'masquées' : 'masquée'} faute de place, toutes dans le tableau.`;
    }
    if (portee && portee.bornes) {
      const [min, max] = portee.bornes;
      const bonus = portee.bonus
        ? ` (${portee.bonus > 0 ? '+' : ''}${portee.bonus} du build)` : '';
      details += ` Portée ${min === max ? min : `${min} à ${max}`}${bonus}.`;
      if (!portee.ligne_de_vue) details += ' Sans ligne de vue.';
      if (outilActif() === 'visee') details += ' Survolez une case pour voir la zone avant de viser.';
    }
    $('detailsZone').textContent = details;

    detailler(z);
    remplirFiche();
  }

  function detailler(z) {
    const hote = $('detailZone');
    if (!z) { hote.innerHTML = ''; return; }
    if (z.refus) {
      hote.innerHTML = '<p class="bandeau souci" style="margin-top:.6rem">'
        + "La forme de cette zone n'est pas dessinée : son nombre de cases est "
        + "mesuré en jeu, pas son dessin.</p>";
      return;
    }
    if (!z.touches.length) {
      hote.innerHTML = placement.ennemis.length
        ? `<p class="aide">Sa zone (${z.cases.length} `
          + `${pluriel(z.cases.length, 'case')}) ne couvre aucun de vos ennemis.</p>`
        : '<p class="aide">Posez un ennemi dans la zone pour voir ce qu\'il prend.</p>';
      return;
    }
    // Un sort qui frappe par l'état qu'il pose (poison, glyphe) a ses coups dans un
    // tableau par état, avec le moment où ils tombent.
    const direct = z.touches.some((t) => t.normal);
    const etats = (z.etats || []).filter((e) => e.touches.length);
    if (!direct && !etats.length) {
      hote.innerHTML = `<p class="aide">${z.touches.length} ${pluriel(z.touches.length, 'ennemi')}`
        + ' dans la zone. Ce sort ne frappe pas au lancer.</p>';
      return;
    }
    const plage = (f) => (f ? `${nb(f[0])} à ${nb(f[1])}` : '?');
    // Un état qui ne critique jamais n'a pas de colonne Critique ; celui qui
    // critique à un autre taux le dit dans l'en-tête de sa colonne.
    const tableau = (touches, sur, critiques, taux) => `<table class="mini selectionnable" style="margin-top:.6rem">
      <thead><tr><th>Ennemi</th><th>Éloignement</th><th>Taux</th>
        <th${sur ? ' class="attenue"' : ''}>Dégâts</th>${
          critiques ? `<th${sur ? '' : ' class="attenue"'}>Critique${
            taux == null ? '' : ` (${taux} %)`}</th>` : ''}</tr></thead>
      <tbody>${touches.map((t) => `<tr>
        <td>N° ${(t.indice || 0) + 1}</td>
        <td>${t.eloignement === 0 ? "Sur l'impact" : `${t.eloignement} ${pluriel(t.eloignement, 'case')}`}</td>
        <td>${t.taux} %</td>
        <td${sur ? ' class="attenue"' : ''}>${plage(t.normal)}</td>
        ${critiques ? `<td${sur ? '' : ' class="attenue"'}>${plage(t.critique)}</td>` : ''}
      </tr>`).join('')}</tbody></table>`;
    let html = direct ? tableau(z.touches, critiqueSur(z), z.touches.some((t) => t.critique)) : '';
    etats.forEach((e) => {
      const quand = e.declencheurs.map(phraseDeclencheur).join(', et ');
      html += `<p class="aide" style="margin-top:.6rem">${majuscule(`${direct ? 'puis ' : ''}${quand}`)} :</p>`
        + tableau(e.touches, e.critique >= 100, e.touches.some((t) => t.critique),
          e.critique !== z.critique ? e.critique : null);
    });
    // Sous les tableaux, seulement ce qui s'applique au sort ; le palier de base ne
    // se dit que pour un sort à charges.
    const conditions = [
      avecEquipement() ? '' : 'niveau 200 sans équipement',
      z.charges ? 'sans charge accumulée' : '',
      z.plafond ? `${z.plafond} ${pluriel(z.plafond, 'cible')} au plus` : '',
    ].filter(Boolean);
    hote.innerHTML = html
      + (conditions.length ? `<p class="aide">${majuscule(conditions.join(' · '))}</p>` : '');
  }

  // La zone s'affiche aussi sans ennemi posé.
  async function interroger() {
    if (!sortChoisi) { reponse = null; peindre(); return; }
    try {
      reponse = await appeler('zones', {
        ...buildPour(),
        // Les bonus de Dofus que le joueur a écartés dans l'onglet Rotation.
        bonus_ecartes: avecEquipement() ? (etat.bonusEcartes || []) : [],
        deck: [sortChoisi],
        placement: {
          lanceur: placement.lanceur,
          visee: placement.visee,
          ennemis: placement.ennemis,
          ...pourLeServeur(plateau),
        },
      });
    } catch (e) {
      reponse = null;
      $('etatZone').textContent = e.message;
    }
    peindre();
  }

  peindre();
  interroger();
}
