// KrozBoom : les bombes du Roublard, leurs murs et leurs explosions.
//
// Trois bombes au plus ; deux bombes du même élément alignées à sept cases au
// plus tendent un mur ; chaque bombe explose en cercle de deux cases.
//
// Les règles restent au serveur (`crates/app/src/bombes.rs`) : murs, combos
// cumulés, souffles et dégâts. La page pose des pièces et dessine ce qu'on lui
// répond.

import { contourZone, dessiner, losange, memeCase, rafraichirApercu, versEcran } from '../commun/damier.js';
import { placer } from '../commun/etiquettes.js';
import { COULEUR_ELEMENT, nb, pluriel } from '../commun/format.js';
import {
  HAUTEUR_POUTCH, fantome, interdit, poutchs, retrait,
} from '../commun/pions.js';
import {
  bilanDuRecalage, chargerCartes, choisir, garderLeSol, plateauActif, pourLeServeur,
  selecteur,
} from '../commun/plateaux.js';

const ROUBLARD = 13;
const BOMBES_MAX = 3;
const ROMAINS = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X', 'XI', 'XII', 'XIII', 'XIV', 'XV'];

/// Les quatre bombes, par le sort qui les pose : son nom, son élément, son
/// icône.
const BOMBES = {
  feu: { nom: 'Explobombe', element: 'Feu', sort: 13444 },
  air: { nom: 'Tornabombe', element: 'Air', sort: 13435 },
  eau: { nom: 'Bombe à Eau', element: 'Eau', sort: 13436 },
  terre: { nom: 'Sismobombe', element: 'Terre', sort: 13491 },
};

const cle = (c) => `${c[0]},${c[1]}`;
const couleur = (el) => COULEUR_ELEMENT[BOMBES[el].element];
const plage = (f) => (f ? `${nb(f[0])} à ${nb(f[1])}` : '–');
const milieu = (f) => Math.round((f[0] + f[1]) / 2);

/// Une bombe sur sa case : l'icône de son sort, son combo en pastille, un
/// anneau si elle est choisie.
function jetonBombe([x, y], element, combo, choisie) {
  const [cx, cy] = versEcran(x, y);
  const romain = ROMAINS[combo - 1];
  const l = romain.length > 2 ? 21 : romain.length > 1 ? 17 : 13;
  return (choisie ? `<polygon class="bombe-choisie" points="${losange(x, y)}"/>` : '')
    + `<image class="bombe-icone" href="icons/${BOMBES[element].sort}.png" x="${cx - 13}" y="${cy - 22}" width="26" height="26"/>`
    + `<rect class="bombe-combo-fond" fill="${couleur(element)}" x="${cx + 5}" y="${cy - 3}" width="${l}" height="13" rx="6.5"/>`
    + `<text class="bombe-combo" x="${cx + 5 + l / 2}" y="${cy + 6.5}">${romain}</text>`;
}

export async function rendre(cible, etat, { appeler }) {
  await chargerCartes(etat, appeler);
  let plateau = plateauActif(etat);
  // Un exemple à l'ouverture : deux Explobombes qui tendent un mur, un ennemi
  // dedans et un autre dans le souffle.
  const boom = etat.boom || (etat.boom = {
    bombes: [{ case: [-2, 0], element: 'feu', combo: 3 }, { case: [2, 0], element: 'feu', combo: 2 }],
    ennemis: [[0, 0], [2, 2]],
    element: 'feu',
    melee: false,
    choisie: 0,
  });
  let reponse = null;
  let message = '';

  const buildRoublard = () => Boolean(etat.build) && etat.resolu && etat.resolu.class === ROUBLARD;
  const avecBuild = () => buildRoublard() && etat.boomAvecBuild !== false;

  function recalerBoom() {
    const retires = garderLeSol(boom.ennemis, plateau) + garderLeSol(boom.bombes, plateau, (b) => b.case);
    if (boom.choisie != null && boom.choisie >= boom.bombes.length) boom.choisie = boom.bombes.length ? 0 : null;
    return bilanDuRecalage(0, retires);
  }
  message = recalerBoom();

  cible.innerHTML = `
    <section class="outil">
      <p class="sous">Posez jusqu'à trois bombes et vos ennemis.</p>

      <div class="vue-damier">
        <div>
          <div class="barre-outils">${selecteur('choixPlateauBoom', etat)}</div>
          <div class="barre-outils">
            <label class="segment"><input type="radio" name="outilBoom" value="bombe" checked> Bombe</label>
            <label class="segment"><input type="radio" name="outilBoom" value="ennemi"> Ennemi</label>
            <button type="button" class="action discret" id="viderBoom">Vider</button>
          </div>
          <div class="barre-outils">
            <label class="choix-bombe" for="elementBoom">Bombe à poser
              <select id="elementBoom">
                ${Object.entries(BOMBES).map(([id, b]) => `<option value="${id}"${boom.element === id ? ' selected' : ''}>${b.nom} (${b.element})</option>`).join('')}
              </select>
            </label>
          </div>
          <div class="plateau" id="plateauBoom"></div>
          <p class="aide" id="etatBoom"></p>
          <details class="plus-loin">
            <summary>Pour aller plus loin</summary>
            <p class="aide">Deux bombes du même élément alignées forment un mur. Les bombes à
              deux cases ou moins de celle qui explose, et celles de son mur, explosent avec
              elle, de proche en proche.</p>
            <p class="aide" id="legendeBoom"></p>
          </details>
        </div>

        <div class="carte">
          <h3>Bombes posées</h3>
          <label class="bascule" for="avecBuildBoom">
            <input type="checkbox" id="avecBuildBoom">
            <span>Avec le build importé</span>
          </label>
          <p class="aide" id="aideBuildBoom"></p>
          <p class="libelle-groupe">Position du Roublard</p>
          <div class="barre-outils">
            <label class="segment"><input type="radio" name="porteeBoom" value="distance"${boom.melee ? '' : ' checked'}> Distance</label>
            <label class="segment"><input type="radio" name="porteeBoom" value="melee"${boom.melee ? ' checked' : ''}> Mêlée</label>
          </div>
          <p class="libelle-groupe">Plombages ce tour</p>
          <div class="barre-outils">
            ${[0, 1, 2].map((n) => `<label class="segment"><input type="radio" name="plombageBoom" value="${n}"${(boom.plombages || 0) === n ? ' checked' : ''}> ${n ? `${n} Plombage${n > 1 ? 's' : ''}` : 'Sans Plombage'}</label>`).join('')}
          </div>
          <p class="libelle-groupe">Bombe à faire exploser</p>
          <div class="barre-outils" id="exploseBoom"></div>
          <div id="bombesBoom"></div>
          <div id="detailBoom"></div>
        </div>
      </div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);
  const outilActif = () =>
    (cible.querySelector('input[name=outilBoom]:checked') || {}).value || 'bombe';
  const bombeEn = (c) => boom.bombes.findIndex((b) => memeCase(b.case, c));
  const estEnnemi = (c) => boom.ennemis.some((e) => memeCase(e, c));

  function majCaseBuild() {
    const caseBuild = $('avecBuildBoom');
    caseBuild.disabled = !buildRoublard();
    caseBuild.checked = avecBuild();
    // Sans build de Roublard, le calcul part d'un niveau 200 sans équipement ;
    // « Pour aller plus loin », sous le tableau, le dit déjà.
    $('aideBuildBoom').textContent = buildRoublard()
      ? '' : "Importez un build de Roublard dans l'onglet Équipement.";
  }
  majCaseBuild();

  function cliquer(x, y) {
    const ici = [x, y];
    message = '';
    if (outilActif() === 'bombe') {
      const i = bombeEn(ici);
      if (i >= 0) {
        boom.bombes.splice(i, 1);
        boom.choisie = boom.bombes.length ? Math.min(boom.choisie || 0, boom.bombes.length - 1) : null;
        if (boom.explose === i) boom.explose = null;
        else if (boom.explose != null && boom.explose > i) boom.explose -= 1;
      } else if (estEnnemi(ici)) {
        message = ' Un ennemi se tient là : une bombe se pose sur une case libre.';
      } else if (boom.bombes.length >= BOMBES_MAX) {
        message = ` ${BOMBES_MAX} bombes au plus, comme en jeu : retirez-en une, en cliquant dessus, avant d'en poser une autre.`;
      } else {
        boom.bombes.push({ case: ici, element: boom.element, combo: 1 });
        boom.choisie = boom.bombes.length - 1;
      }
    } else if (bombeEn(ici) >= 0) {
      message = ' Une bombe occupe la case : un ennemi se pose sur une case libre.';
    } else {
      const i = boom.ennemis.findIndex((e) => memeCase(e, ici));
      if (i >= 0) boom.ennemis.splice(i, 1);
      else boom.ennemis.push(ici);
    }
    interroger();
  }

  /// Ce qu'un clic poserait sous le curseur : la bombe de l'élément choisi ou
  /// un Poutch, en transparence ; une croix si le clic retirerait la pièce, un
  /// contour rouge si la case ne la prend pas.
  function aPoser(x, y) {
    const ici = [x, y];
    if (outilActif() === 'bombe') {
      if (bombeEn(ici) >= 0) return retrait(ici);
      if (estEnnemi(ici) || boom.bombes.length >= BOMBES_MAX) return interdit(ici);
      return fantome(jetonBombe(ici, boom.element, 1, false));
    }
    if (bombeEn(ici) >= 0) return interdit(ici);
    return estEnnemi(ici) ? retrait(ici) : fantome(poutchs([{ case: ici, rang: boom.ennemis.length + 1 }]));
  }

  $('elementBoom').addEventListener('change', () => {
    boom.element = $('elementBoom').value;
    rafraichirApercu($('plateauBoom'));
  });
  cible.querySelectorAll('input[name=outilBoom]').forEach((radio) => {
    radio.addEventListener('change', () => rafraichirApercu($('plateauBoom')));
  });
  cible.querySelectorAll('input[name=porteeBoom]').forEach((radio) => {
    radio.addEventListener('change', () => {
      boom.melee = radio.value === 'melee' && radio.checked;
      interroger();
    });
  });
  cible.querySelectorAll('input[name=plombageBoom]').forEach((radio) => {
    radio.addEventListener('change', () => {
      if (radio.checked) boom.plombages = Number(radio.value);
      interroger();
    });
  });
  $('avecBuildBoom').addEventListener('change', () => {
    etat.boomAvecBuild = $('avecBuildBoom').checked;
    interroger();
  });
  $('viderBoom').addEventListener('click', () => {
    boom.bombes = [];
    boom.ennemis = [];
    boom.choisie = null;
    boom.explose = null;
    message = '';
    interroger();
  });
  $('choixPlateauBoom').addEventListener('change', () => {
    choisir(etat, $('choixPlateauBoom').value);
    plateau = plateauActif(etat);
    message = recalerBoom();
    interroger();
  });

  /// Le choix de la bombe qui saute : toutes ensemble, ou l'une d'elles et sa
  /// réaction en chaîne.
  function choixExplosion() {
    const hote = $('exploseBoom');
    const choix = [null, ...boom.bombes.map((_, i) => i)];
    hote.innerHTML = choix.map((i) => `<label class="segment"><input type="radio" name="exploseBoom" value="${i ?? ''}"${(boom.explose ?? null) === i ? ' checked' : ''}> ${i == null ? 'Toutes ensemble' : `Bombe n° ${i + 1}`}</label>`).join('');
    hote.querySelectorAll('input[name=exploseBoom]').forEach((radio) => {
      radio.addEventListener('change', () => {
        if (radio.checked) boom.explose = radio.value === '' ? null : Number(radio.value);
        interroger();
      });
    });
  }

  /// La liste des bombes : leur combo se choisit ici, et ce que les autres
  /// leur ajoutent se lit à côté.
  function listerBombes() {
    const hote = $('bombesBoom');
    if (!boom.bombes.length) {
      hote.innerHTML = '<p class="aide">Aucune bombe posée.</p>';
      return;
    }
    const vues = (reponse && reponse.bombes) || [];
    hote.innerHTML = boom.bombes.map((b, i) => {
      const v = vues[i];
      const cumuls = v ? `${v.saute ? `Explosion +${v.combo_explosion} %` : 'Ne saute pas'}${v.en_mur ? `, mur +${v.combo_mur} %` : ''}` : '';
      return `<div class="bombe-ligne${boom.choisie === i ? ' choisie' : ''}" data-bombe="${i}">
        <img src="icons/${BOMBES[b.element].sort}.png" alt="" width="22" height="22">
        <span class="bombe-nom" style="color:${couleur(b.element)}">${BOMBES[b.element].nom}</span>
        <label>Combo <select data-combo="${i}" aria-label="Combo de la bombe ${i + 1}">
          ${ROMAINS.map((r, k) => `<option value="${k + 1}"${b.combo === k + 1 ? ' selected' : ''}>${r}</option>`).join('')}
        </select></label>
        <span class="aide">${cumuls}</span>
      </div>`;
    }).join('');
    hote.querySelectorAll('select[data-combo]').forEach((s) => {
      s.addEventListener('change', () => {
        const i = Number(s.dataset.combo);
        boom.bombes[i].combo = Number(s.value);
        boom.choisie = i;
        message = '';
        interroger();
      });
    });
  }

  function detailler() {
    const hote = $('detailBoom');
    if (!reponse) { hote.innerHTML = ''; return; }
    if (!boom.ennemis.length) {
      hote.innerHTML = '<p class="aide">Posez un ennemi pour voir ce qu\'il prend.</p>';
      return;
    }
    const plombages = reponse.plombages || 0;
    const lignes = reponse.ennemis.map((e) => {
      const m = e.mur;
      const teinte = m ? ` style="color:${COULEUR_ELEMENT[BOMBES[m.element].element]}"` : '';
      return `<tr>
        <td>N° ${e.indice + 1}</td>
        <td>${plage(e.explosion)}</td>
        <td${teinte}>${m ? plage(m.tour) : '–'}</td>
        <td${teinte}>${m ? plage(m.hors_tour) : '–'}</td>
        ${plombages ? `<td${teinte}>${plage(e.plombage)}</td>` : ''}
      </tr>`;
    }).join('');
    hote.innerHTML = `<table class="mini selectionnable" style="margin-top:.6rem">
      <thead><tr><th>Ennemi</th><th>Explosions</th><th>Mur, votre tour</th><th>Mur, hors de votre tour</th>${plombages ? `<th>${plombages} Plombage${plombages > 1 ? 's' : ''}</th>` : ''}</tr></thead>
      <tbody>${lignes}</tbody></table>
      <details class="plus-loin">
      <summary>Pour aller plus loin</summary>
      <p class="aide">${avecBuild() ? 'Avec le build importé' : 'Niveau 200 sans équipement'} ; ${
        reponse.explose == null ? 'les explosions de toutes les bombes posées, parties ensemble'
          : `l'explosion de la bombe n° ${reponse.explose + 1} et de celles qu'elle emporte`}. Un mur prend
        vos % de dommages aux sorts (${nb(reponse.pourcents_des_murs.sorts)} %), de
        ${reponse.melee ? 'mêlée' : 'distance'} (${nb(reponse.pourcents_des_murs.portee)} %) et finaux
        (${nb(reponse.pourcents_des_murs.finaux)} %, Dofus compris) ; une explosion, aucun.</p>
      <p class="aide">Le mur frappe la cible au début de chacun de ses tours, hors de votre tour ;
        à votre tour, quand il se forme sur elle, qu'un de vos sorts l'y déplace ou que Plombage
        le redéclenche.</p>
      </details>`;
  }

  function peindre() {
    const murs = new Map();
    let calques = '';
    ((reponse && reponse.bombes) || []).filter((b) => b.saute !== false).forEach((b) => {
      const cases = b.explosion.map((c) => [c[0], c[1]]);
      calques += cases.map((c) => `<polygon class="souffle-fond" fill="${couleur(b.element)}" points="${losange(c[0], c[1])}"/>`).join('')
        + `<path class="souffle-contour" stroke="${couleur(b.element)}" d="${contourZone(cases)}"/>`;
    });
    ((reponse && reponse.murs) || []).forEach((m) => {
      m.cases.forEach((c) => murs.set(cle(c), m.element));
      calques += m.cases.map((c) => `<polygon class="mur-bombe" fill="${couleur(m.element)}" points="${losange(c[0], c[1])}"/>`).join('');
    });

    let jetons = boom.bombes.map((b, i) => jetonBombe(b.case, b.element, b.combo, boom.choisie === i)).join('');
    jetons += poutchs(boom.ennemis.map((e, i) => ({ case: e, rang: i + 1 })));

    // Sur la carte, l'explosion de chaque ennemi, ou le mur à votre tour s'il
    // n'est dans aucun souffle.
    let masquees = 0;
    if (reponse) {
      const items = reponse.ennemis.filter((e) => e.explosion || e.mur).map((e) => ({
        case: e.case,
        texte: nb(milieu(e.explosion || e.mur.tour)),
        couleur: e.explosion ? 'var(--encre)' : COULEUR_ELEMENT[BOMBES[e.mur.element].element],
      }));
      const pose = placer(items, [
        ...boom.ennemis.map((e) => [e[0], e[1], HAUTEUR_POUTCH]), ...boom.bombes.map((b) => b.case)]);
      jetons += pose.svg;
      masquees = pose.masquees;
    }

    dessiner($('plateauBoom'), {
      plateau,
      case: (x, y) => {
        const ici = [x, y];
        const i = bombeEn(ici);
        const mur = murs.get(cle(ici));
        return {
          classes: estEnnemi(ici) ? ['ennemi'] : [],
          titre: i >= 0 ? `${BOMBES[boom.bombes[i].element].nom}, combo ${ROMAINS[boom.bombes[i].combo - 1]}`
            : estEnnemi(ici) ? 'Ennemi'
            : mur ? `Mur ${BOMBES[mur].element === 'Air' ? "d'Air" : BOMBES[mur].element === 'Eau' ? "d'Eau" : `de ${BOMBES[mur].element}`}`
            : '',
        };
      },
      calques,
      jetons,
      surClic: cliquer,
      apercu: aPoser,
    });

    const n = boom.bombes.length;
    const texte = `${n} ${pluriel(n, 'bombe')} sur ${BOMBES_MAX}, ${boom.ennemis.length} ${pluriel(boom.ennemis.length, 'ennemi')}.`;
    $('etatBoom').textContent = texte + message;
    let legende = '';
    if (reponse && reponse.murs.length) legende += 'En aplat les murs, en pointillé le souffle des bombes qui sautent.';
    else if (n) legende += 'En pointillé le souffle des bombes qui sautent.';
    if (reponse && reponse.ennemis.some((e) => e.explosion || e.mur)) {
      legende += ' Les nombres sont la moyenne des explosions, ou du mur à votre tour hors des souffles.';
    }
    if (masquees) legende += ` ${masquees} ${pluriel(masquees, 'étiquette')} ${masquees > 1 ? 'masquées' : 'masquée'} faute de place, toutes dans le tableau.`;
    $('legendeBoom').textContent = legende.trim();
    choixExplosion();
    listerBombes();
    detailler();
  }

  async function interroger() {
    try {
      reponse = await appeler('bombes', {
        ...(avecBuild() ? { build: etat.build, bonus_ecartes: etat.bonusEcartes || [] } : {}),
        bombes: boom.bombes,
        ennemis: boom.ennemis,
        melee: boom.melee,
        plombages: boom.plombages || 0,
        ...(boom.explose != null ? { explose: boom.explose } : {}),
        ...pourLeServeur(plateau),
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
