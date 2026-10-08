// KrozPortal : par où ressort un sort projeté dans les portails de l'Éliotrope,
// le sien ou celui d'un allié, et ce qu'il inflige en ressortant. La règle reste
// au serveur (`crates/app/src/portails.rs`) ; la page pose des coordonnées et
// dessine la réponse. L'entrée se vise en vue de l'Éliotrope, et le sort ne
// ressort que vers du sol en vue de la sortie.

import { contourZone, dessiner, losange, memeCase, versEcran } from '../commun/damier.js';
import { placer } from '../commun/etiquettes.js';
import { COULEUR_ELEMENT, echapper, majuscule, nb, pluriel } from '../commun/format.js';
import {
  HAUTEUR_POUTCH, fantome, interdit, jetonLettre, poutchs, retrait,
} from '../commun/pions.js';
import {
  bilanDuRecalage, chargerCartes, choisir, garderLeSol, plateauActif, pourLeServeur,
  recaler, selecteur,
} from '../commun/plateaux.js';

const ELIOTROPE = 16;
/// Le portail tel qu'on le voit en jeu, aplati à la perspective du sol : deux
/// fois plus large que haut, comme une case du damier.
const IMAGE_PORTAIL = 'plateau/portail.png';
/// Quatre portails au plus : le cinquième fait disparaître le plus ancien.
const MAX_PORTAILS = 4;

export async function rendre(cible, etat, { appeler }) {
  // Un exemple posé d'emblée, pour que l'écran montre ce qu'il fait avant le
  // premier clic : l'Éliotrope vise l'entrée à trois cases, le sort ressort
  // trois cases après la sortie, sur l'ennemi qui s'y tient.
  const plan = etat.portails || (etat.portails = {
    eliotrope: [-5, 1],
    portails: [[-2, 1], [2, -3]],
    entree: [-2, 1],
    ennemis: [[5, -3]],
  });
  const MAX_ENNEMIS = (etat.classes && etat.classes.limites && etat.classes.limites.ennemis) || 1;
  let reponse = null;
  let message = '';
  await chargerCartes(etat, appeler);
  let plateau = plateauActif(etat);

  /// Ce qui n'est plus sur le sol du plateau en revient : l'Éliotrope sur la
  /// case libre la plus proche, les portails et les ennemis retirés.
  function recalerPlan() {
    const retires = garderLeSol(plan.portails, plateau) + garderLeSol(plan.ennemis, plateau);
    if (plan.entree && !plan.portails.some((q) => memeCase(q, plan.entree))) {
      plan.entree = plan.portails[0] || null;
    }
    const ici = recaler(plan.eliotrope, plateau, [...plan.portails, ...plan.ennemis]);
    const deplace = ici && !memeCase(ici, plan.eliotrope) ? 1 : 0;
    if (ici) plan.eliotrope = ici;
    return bilanDuRecalage(deplace, retires);
  }
  message = recalerPlan();

  // La classe du lanceur se choisit : un allié projette aussi ses sorts dans les
  // portails, avec le même bonus. Par défaut, la classe du build chargé, sinon
  // l'Éliotrope.
  const classes = (etat.classes && etat.classes.classes) || [];
  const classeDuBuild = etat.resolu && etat.resolu.class;
  let classeId = etat.portailsClasse || classeDuBuild || ELIOTROPE;
  const sortsDe = (id) => ((classes.find((c) => c.id === id) || {}).spells || [])
    .filter((s) => (s.elements || []).length)
    .sort((a, b) => a.name.localeCompare(b.name, 'fr', { sensitivity: 'base' }));
  let sorts = sortsDe(classeId);
  let sortChoisi = (sorts.find((s) => s.id === etat.portailsSort) || sorts[0] || {}).id || null;
  /// Le lanceur, nommé selon sa classe : l'Éliotrope lui-même, ou l'allié.
  const lanceur = () => (classeId === ELIOTROPE
    ? { le: "l'Éliotrope", du: "de l'Éliotrope", lettre: 'É' }
    : { le: 'le lanceur', du: 'du lanceur', lettre: 'V' });

  const buildDeLaClasse = () => Boolean(etat.build) && classeId === classeDuBuild;
  const avecEquipement = () => buildDeLaClasse() && etat.portailsAvecBuild !== false;
  const buildPour = () => (avecEquipement() ? etat.build : { class: classeId, level: 200 });

  cible.innerHTML = `
    <section class="outil">
      <p class="sous">Placez le lanceur, les portails et vos ennemis, puis choisissez le
        portail d'entrée.</p>

      <div class="vue-damier">
        <div>
          <div class="barre-outils">${selecteur('choixPlateauPortails', etat)}</div>
          <div class="barre-outils">
            <label class="segment"><input type="radio" name="outilPortail" value="portail" checked> Portail</label>
            <label class="segment"><input type="radio" name="outilPortail" value="entree"> Projeter dans…</label>
            <label class="segment"><input type="radio" name="outilPortail" value="ennemi"> Ennemi</label>
            <label class="segment"><input type="radio" name="outilPortail" value="eliotrope"> Lanceur</label>
            <button type="button" class="action discret" id="viderPortails">Retirer les portails</button>
          </div>
          <div class="plateau" id="plateauPortails"></div>
          <p class="aide" id="etatPortails"></p>
          <details class="plus-loin" id="plusLoinPortails" hidden>
            <summary>Pour aller plus loin</summary>
            <p class="aide" id="detailsPortails"></p>
          </details>
        </div>

        <div class="carte">
          <h3>Sort projeté</h3>
          <select id="classePortail" aria-label="Classe du lanceur">
            ${classes.map((c) => `<option value="${c.id}" ${c.id === classeId ? 'selected' : ''}>${echapper(c.label)}</option>`).join('')}
          </select>
          <select id="sortPortail" aria-label="Sort projeté"></select>
          <label class="bascule" for="avecBuildPortail">
            <input type="checkbox" id="avecBuildPortail">
            <span>Avec le build importé</span>
          </label>
          <p class="aide palier-aide" id="aideBuildPortail"></p>
          <div id="bonusPortail"></div>
          <div id="detailPortail"></div>
        </div>
      </div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);
  const outilActif = () =>
    (cible.querySelector('input[name=outilPortail]:checked') || {}).value || 'portail';
  const estPortail = (c) => plan.portails.some((p) => memeCase(p, c));
  const estEnnemi = (c) => plan.ennemis.some((e) => memeCase(e, c));

  function remplirSorts() {
    $('sortPortail').innerHTML = sorts
      .map((s) => `<option value="${echapper(s.id)}" ${s.id === sortChoisi ? 'selected' : ''}>${echapper(s.name)}</option>`)
      .join('');
  }
  remplirSorts();

  /// La case suit la classe choisie : cochable avec un build de cette classe,
  /// grisée sinon, et la ligne d'aide dit quoi faire pour qu'elle le soit.
  function majCaseBuild() {
    const caseBuild = $('avecBuildPortail');
    caseBuild.disabled = !buildDeLaClasse();
    caseBuild.checked = avecEquipement();
    $('aideBuildPortail').textContent = !etat.build
      ? "Importez un build dans l'onglet Équipement."
      : !buildDeLaClasse()
        ? 'Choisissez la classe du build importé.'
        : '';
  }
  majCaseBuild();

  function cliquer(x, y) {
    const ici = [x, y];
    const quoi = outilActif();
    message = '';
    if (quoi === 'portail') {
      const i = plan.portails.findIndex((p) => memeCase(p, ici));
      if (i >= 0) {
        plan.portails.splice(i, 1);
        if (memeCase(plan.entree, ici)) plan.entree = plan.portails[0] || null;
      } else if (memeCase(plan.eliotrope, ici) || estEnnemi(ici)) {
        message = ' Un portail se pose sur une case libre.';
      } else {
        plan.portails.push(ici);
        if (plan.portails.length > MAX_PORTAILS) {
          const retire = plan.portails.shift();
          message = ' Quatre portails au plus : le plus ancien a disparu.';
          if (memeCase(plan.entree, retire)) plan.entree = null;
        }
        if (!plan.entree) plan.entree = ici;
      }
    } else if (quoi === 'entree') {
      if (estPortail(ici)) plan.entree = ici;
      else message = ' Cliquez un portail : le sort se projette dans l\'un d\'eux.';
    } else if (quoi === 'ennemi') {
      const i = plan.ennemis.findIndex((e) => memeCase(e, ici));
      if (i >= 0) plan.ennemis.splice(i, 1);
      else if (estPortail(ici) || memeCase(plan.eliotrope, ici)) {
        message = ' Cette case est déjà prise.';
      } else if (plan.ennemis.length >= MAX_ENNEMIS) {
        message = ` ${MAX_ENNEMIS} ennemis au plus : retirez-en un, en cliquant dessus, avant d'en poser un autre.`;
      } else plan.ennemis.push(ici);
    } else if (quoi === 'eliotrope') {
      if (estPortail(ici) || estEnnemi(ici)) message = ' Cette case est déjà prise.';
      else plan.eliotrope = ici;
    }
    interroger();
  }

  cible.querySelectorAll('input[name=outilPortail]').forEach((radio) => {
    radio.addEventListener('change', () => peindre());
  });
  $('choixPlateauPortails').addEventListener('change', () => {
    choisir(etat, $('choixPlateauPortails').value);
    plateau = plateauActif(etat);
    message = recalerPlan();
    interroger();
  });
  $('viderPortails').addEventListener('click', () => {
    plan.portails = [];
    plan.entree = null;
    message = '';
    interroger();
  });
  $('classePortail').addEventListener('change', () => {
    classeId = Number($('classePortail').value);
    etat.portailsClasse = classeId;
    sorts = sortsDe(classeId);
    sortChoisi = (sorts.find((s) => s.id === etat.portailsSort) || sorts[0] || {}).id || null;
    remplirSorts();
    majCaseBuild();
    interroger();
  });
  $('sortPortail').addEventListener('change', () => {
    sortChoisi = $('sortPortail').value;
    etat.portailsSort = sortChoisi;
    interroger();
  });
  $('avecBuildPortail').addEventListener('change', () => {
    etat.portailsAvecBuild = $('avecBuildPortail').checked;
    interroger();
  });

  /// Pourquoi une case ne se vise pas, en une phrase.
  function phraseDeRefus(refus, quoi, depuis) {
    if (!refus) return '';
    const { motif, distance: d, limite } = refus;
    if (motif === 'trop_loin') {
      return ` ${quoi} est hors de portée ${depuis} : ${d} ${pluriel(d, 'case')}, ${limite} au plus.`;
    }
    if (motif === 'trop_pres') {
      return ` ${quoi} est trop près ${depuis} : ${d} ${pluriel(d, 'case')}, ${limite} au moins.`;
    }
    if (motif === 'hors_axe') return ` ${quoi} n'est pas dans l'axe du sort ${depuis}.`;
    if (motif === 'case_vide') return ` Ce sort exige une cible sur ${quoi.toLowerCase()}.`;
    if (motif === 'case_occupee') return ` Ce sort exige une case libre.`;
    if (motif === 'hors_vue') return ` ${quoi} n'est pas en ligne de vue ${depuis} : un mur ou un corps le cache.`;
    return '';
  }

  /// Pourquoi le sort ne ressort pas du portail de sortie.
  function phraseDArrivee(motif) {
    if (motif === 'hors_sol') {
      return plateau.carte != null
        ? " Le sort ne ressort pas : il atterrirait sur un mur ou dans un trou."
        : ` Le sort ressortirait hors du damier : rapprochez ${lanceur().le} de l'entrée.`;
    }
    if (motif === 'hors_vue') {
      return " Le sort ne ressort pas : un mur ou un corps coupe la vue entre la sortie et la case d'arrivée.";
    }
    return '';
  }

  const tir = () => (reponse && reponse.tir && (reponse.tir.sorts || [])[0]) || null;

  /// Le portail en image, posé à plat sur sa case.
  const imagePortail = ([x, y]) => {
    const [px, py] = versEcran(x, y);
    return `<image class="portail-image" href="${IMAGE_PORTAIL}" preserveAspectRatio="none"`
      + ` x="${px - 30}" y="${py - 17}" width="60" height="32"/>`;
  };

  /// Ce qu'un clic poserait sous le curseur, avec l'outil actif : la pièce en
  /// transparence, une croix si le clic la retirerait, un contour rouge si la
  /// case ne la prend pas.
  function apercu(x, y) {
    const ici = [x, y];
    const quoi = outilActif();
    const surLanceur = memeCase(plan.eliotrope, ici);
    if (quoi === 'portail') {
      if (estPortail(ici)) return retrait(ici);
      return surLanceur || estEnnemi(ici) ? interdit(ici) : fantome(imagePortail(ici));
    }
    if (quoi === 'entree') {
      if (!estPortail(ici)) return interdit(ici);
      const [px, py] = versEcran(x, y);
      return fantome(`<circle class="portail-rang" cx="${px + 17}" cy="${py - 12}" r="7.5"/>`
        + `<text class="jeton portail-rang-texte" x="${px + 17}" y="${py - 12}">1</text>`);
    }
    if (quoi === 'ennemi') {
      if (estEnnemi(ici)) return retrait(ici);
      if (estPortail(ici) || surLanceur || plan.ennemis.length >= MAX_ENNEMIS) return interdit(ici);
      return fantome(poutchs([{ case: ici, rang: plan.ennemis.length + 1 }]));
    }
    if (estPortail(ici) || estEnnemi(ici)) return interdit(ici);
    return surLanceur ? '' : fantome(jetonLettre(ici, 'lanceur', lanceur().lettre));
  }
  const critiqueSur = (z) => Boolean(z) && z.critique >= 100;

  function peindre() {
    const trajet = (reponse && reponse.trajet) || [];
    const sortie = reponse && reponse.sortie;
    const arrivee = reponse && reponse.arrivee;
    const z = tir();
    const rang = (c) => trajet.findIndex((t) => memeCase(t, c));
    const dansZone = new Set(((z && z.cases) || []).map((c) => `${c[0]},${c[1]}`));
    // En bleu, la portée du sort vue de l'Éliotrope : l'entrée doit y être.
    const depuis = (reponse && reponse.depuis_eliotrope) || null;
    const portee = new Set(((depuis && depuis.cases) || []).map((c) => `${c[0]},${c[1]}`));
    const masquee = new Set(((depuis && depuis.masquees) || []).map((c) => `${c[0]},${c[1]}`));
    const refusArrivee = reponse && reponse.arrivee_refus;

    let calques = '';
    if (z && z.cases && z.cases.length) {
      const couleur = COULEUR_ELEMENT[z.element] || 'var(--accent)';
      calques += z.cases
        .map((c) => `<polygon class="zone-fond" fill="${couleur}" points="${losange(c[0], c[1])}"/>`).join('')
        + `<path class="zone-contour" stroke="${couleur}" d="${contourZone(z.cases)}"/>`;
    }
    // Refusée, l'arrivée reste dessinée, en pointillé : on voit où le sort
    // serait tombé, et ce qui l'en empêche.
    const refusee = refusArrivee ? ' refusee' : '';
    if (arrivee) {
      calques += `<polygon class="visee-trait${refusee}" points="${losange(arrivee[0], arrivee[1])}"/>`;
    }
    // L'Éliotrope vise l'entrée ; le sort traverse les portails puis va de la
    // sortie à l'arrivée, du même vecteur.
    if (trajet.length >= 2) {
      const point = (c) => versEcran(c[0], c[1]).join(',');
      calques += `<polyline class="trajet vise" points="${point(plan.eliotrope)} ${point(trajet[0])}"/>`;
      calques += `<polyline class="trajet portail" points="${trajet.map(point).join(' ')}"/>`;
      if (arrivee) {
        calques += `<polyline class="trajet portail${refusee}" points="${point(sortie)} ${point(arrivee)}"/>`;
      }
    }

    let jetons = '';
    const [lx, ly] = versEcran(plan.eliotrope[0], plan.eliotrope[1]);
    jetons += `<text class="jeton lanceur" x="${lx}" y="${ly}">${lanceur().lettre}</text>`;
    // Le portail en image, puis son rang dans le trajet en pastille : 1 pour
    // l'entrée, le dernier pour la sortie, d'une autre couleur.
    plan.portails.forEach((p) => {
      const [px, py] = versEcran(p[0], p[1]);
      const r = rang(p);
      jetons += imagePortail(p);
      if (r >= 0) {
        const sortie = r > 0 && r === trajet.length - 1;
        jetons += `<circle class="portail-rang${sortie ? ' sortie' : ''}" cx="${px + 17}" cy="${py - 12}" r="7.5"/>`
          + `<text class="jeton portail-rang-texte" x="${px + 17}" y="${py - 12}">${r + 1}</text>`;
      }
    });
    jetons += poutchs(plan.ennemis.map((e, i) => ({ case: e, rang: i + 1 })));
    let masquees = 0;
    if (z && !z.refus && (z.touches || []).length) {
      const couleur = COULEUR_ELEMENT[z.element] || 'var(--encre)';
      const items = z.touches
        .map((t) => ({ t, f: critiqueSur(z) && t.critique ? t.critique : t.normal }))
        .filter(({ f }) => f)
        .map(({ t, f }) => ({ case: t.case, texte: nb(Math.round((f[0] + f[1]) / 2)), couleur }));
      const pose = placer(items, [
        ...plan.ennemis.map((e) => [e[0], e[1], HAUTEUR_POUTCH]), plan.eliotrope, ...plan.portails]);
      jetons += pose.svg;
      masquees = pose.masquees;
    }

    dessiner($('plateauPortails'), {
      plateau,
      case: (x, y) => {
        const ici = [x, y];
        const classes = [];
        if (portee.has(`${x},${y}`)) classes.push('portee');
        if (masquee.has(`${x},${y}`)) classes.push('portee-masquee');
        if (dansZone.has(`${x},${y}`)) classes.push('zone');
        if (estPortail(ici)) {
          classes.push('portail');
          if (sortie && memeCase(sortie, ici)) classes.push('sortie');
        }
        if (memeCase(plan.eliotrope, ici)) classes.push('lanceur');
        if (estEnnemi(ici)) classes.push('ennemi');
        const r = rang(ici);
        return {
          classes,
          titre: memeCase(plan.eliotrope, ici) ? majuscule(lanceur().le)
            : r === 0 ? "Portail d'entrée"
            : r > 0 && r === trajet.length - 1 ? 'Portail de sortie'
            : estPortail(ici) ? `Portail n° ${r + 1} du trajet`
            : estEnnemi(ici) ? 'Ennemi'
            : arrivee && memeCase(arrivee, ici) ? "Case d'arrivée du sort" : '',
        };
      },
      calques,
      jetons,
      surClic: cliquer,
      apercu,
    });

    // Le bonus et ce qui empêche le tir.
    const bonus = reponse && reponse.bonus;
    $('bonusPortail').innerHTML = trajet.length >= 2
      ? `<dl><dt>Bonus de projection</dt><dd>+${bonus} % de dommages finaux</dd>
          <dt>Trajet</dt><dd>${trajet.length} portails, du n° 1 au n° ${trajet.length}</dd>
          <dt>Cases parcourues</dt><dd>${reponse.cases} ${pluriel(reponse.cases, 'case')}, 2 % chacune</dd></dl>`
      : '';

    // La consigne et les couleurs à l'écran ; le trajet du sort dans « Pour aller
    // plus loin ».
    let texte = '';
    let details = '';
    if (!plan.portails.length) texte = 'Posez au moins deux portails.';
    else if (plan.portails.length < 2) texte = 'Posez un second portail.';
    else if (!plan.entree) texte = 'Choisissez le portail d\'entrée avec « Projeter dans… ».';
    else if (reponse) {
      const couleurs = [];
      if (portee.size) couleurs.push('En bleu la portée');
      if (masquee.size) couleurs.push(couleurs.length ? 'en gris les cases hors de vue' : 'En gris les cases hors de vue');
      texte = couleurs.length ? `${couleurs.join(', ')}.` : '';
      if (refusArrivee) texte += phraseDArrivee(refusArrivee);
      texte += phraseDeRefus(depuis && depuis.visee, "Le portail d'entrée", lanceur().du);
      details = refusArrivee
        ? `Le sort entre par le portail n° 1 et gagne le n° ${trajet.length}, la sortie.`
        : `Le sort entre par le portail n° 1, ressort par le n° ${trajet.length} et`
          + ' atterrit sur la case cerclée de rouge.';
      if (portee.size) details += ` Le portail d'entrée doit être à portée ${lanceur().du}.`;
      if (masquees) {
        details += ` ${masquees} ${pluriel(masquees, 'étiquette')} `
          + `${masquees > 1 ? 'masquées' : 'masquée'} faute de place, toutes dans le tableau.`;
      }
    }
    $('etatPortails').textContent = (texte + message).trim();
    $('detailsPortails').textContent = details;
    $('plusLoinPortails').hidden = !details;
    detailler(z);
  }

  function detailler(z) {
    const hote = $('detailPortail');
    if (!z) { hote.innerHTML = ''; return; }
    if (z.refus) {
      hote.innerHTML = '<p class="bandeau souci" style="margin-top:.6rem">'
        + "La forme de cette zone n'est pas dessinée : son nombre de cases est mesuré en jeu, pas son dessin.</p>";
      return;
    }
    if (!(z.touches || []).length) {
      hote.innerHTML = plan.ennemis.length
        ? '<p class="aide">Depuis la sortie, la zone ne couvre aucun de vos ennemis.</p>'
        : '<p class="aide">Posez un ennemi pour voir ce qu\'il prend.</p>';
      return;
    }
    if (z.touches.every((t) => !t.normal)) {
      hote.innerHTML = '<p class="aide">Ce sort ne frappe pas au lancer : il pose un état dont'
        + ' les dégâts tombent au début des tours suivants.</p>';
      return;
    }
    const sur = critiqueSur(z);
    const plage = (f) => (f ? `${nb(f[0])} à ${nb(f[1])}` : '?');
    hote.innerHTML = `<table class="mini selectionnable" style="margin-top:.6rem">
      <thead><tr><th>Ennemi</th><th>Éloignement</th><th>Taux</th>
        <th${sur ? ' class="attenue"' : ''}>Dégâts</th><th${sur ? '' : ' class="attenue"'}>Critique</th></tr></thead>
      <tbody>${z.touches.map((t) => `<tr>
        <td>N° ${(t.indice || 0) + 1}</td>
        <td>${t.eloignement === 0 ? "Sur l'impact" : `${t.eloignement} ${pluriel(t.eloignement, 'case')}`}</td>
        <td>${t.taux} %</td>
        <td${sur ? ' class="attenue"' : ''}>${plage(t.normal)}</td>
        <td${sur ? '' : ' class="attenue"'}>${plage(t.critique)}</td>
      </tr>`).join('')}</tbody></table>
      <p class="aide">Bonus de projection compris, au palier de base${
        avecEquipement() ? '' : ', avec un niveau 200 sans équipement'}.</p>`;
  }

  async function interroger() {
    if (plan.portails.length < 2 || !plan.entree) { reponse = null; peindre(); return; }
    try {
      reponse = await appeler('portails', {
        ...pourLeServeur(plateau),
        portails: plan.portails,
        entree: plan.entree,
        eliotrope: plan.eliotrope,
        tir: sortChoisi ? {
          ...buildPour(),
          // Les bonus de Dofus que le joueur a écartés dans l'onglet Rotation.
          bonus_ecartes: etat.bonusEcartes || [],
          deck: [sortChoisi],
          // Le serveur remplace le lanceur par la sortie et la visée par
          // l'arrivée qu'il calcule.
          placement: { lanceur: plan.eliotrope, visee: plan.entree, ennemis: plan.ennemis },
        } : undefined,
      });
    } catch (e) {
      reponse = null;
      message = ` ${e.message}`;
    }
    peindre();
  }

  peindre();
  await interroger();
}
