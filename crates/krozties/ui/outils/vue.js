// KrozSight : ce qu'une case voit, sur le damier vide ou sur une carte de boss.
//
// On choisit sa case, et tout ce qu'elle ne voit pas s'ombre ; au survol,
// l'ombre suit la case survolée avant qu'on la choisisse. Les créatures posées
// coupent la vue comme les murs.
//
// La règle reste au serveur (`crates/grid/src/vue.rs`) : la page pose des
// cases et ombre ce qu'on lui répond.

import { dessiner, marquer, memeCase, rafraichirApercu, versEcran } from '../commun/damier.js';
import { pluriel } from '../commun/format.js';
import {
  fantome, interdit, jetonLettre, poutchs, retrait,
} from '../commun/pions.js';
import {
  bilanDuRecalage, chargerCartes, choisir, garderLeSol, plateauActif, pourLeServeur,
  recaler, selecteur,
} from '../commun/plateaux.js';

const cle = (c) => `${c[0]},${c[1]}`;

export async function rendre(cible, etat, { appeler }) {
  await chargerCartes(etat, appeler);
  let plateau = plateauActif(etat);
  const vue = etat.vue || (etat.vue = { depuis: [0, 0], corps: [] });
  let reponse = null;
  let apercu = null;
  let message = '';
  // Chaque survol numérote sa demande : une réponse arrivée après qu'on a
  // quitté la case, ou survolé la suivante, ne doit rien ombrer.
  let demande = 0;

  /// Ce qui n'est plus sur le sol du plateau en revient : votre case sur la
  /// case libre la plus proche, les créatures retirées.
  function recalerVue() {
    const retires = garderLeSol(vue.corps, plateau);
    const ici = recaler(vue.depuis, plateau, vue.corps);
    const deplace = ici && !memeCase(ici, vue.depuis) ? 1 : 0;
    if (ici) vue.depuis = ici;
    return bilanDuRecalage(deplace, retires);
  }
  message = recalerVue();

  cible.innerHTML = `
    <section class="outil">
      <p class="sous">Choisissez votre case et placez des créatures.</p>

      <div class="vue-damier">
        <div>
          <div class="barre-outils">${selecteur('choixPlateauVue', etat)}</div>
          <div class="barre-outils">
            <label class="segment"><input type="radio" name="outilVue" value="depuis" checked> Votre case</label>
            <label class="segment"><input type="radio" name="outilVue" value="corps"> Une créature</label>
            <button type="button" class="action discret" id="viderVue">Retirer les créatures</button>
          </div>
          <div class="plateau" id="plateauVue"></div>
          <p class="aide" id="etatVue"></p>
        </div>

        <div class="carte">
          <h3>Ligne de vue</h3>
          <div id="bilanVue"></div>
          <details class="plus-loin">
            <summary>Pour aller plus loin</summary>
            <p class="aide">Les murs et les créatures coupent la vue, les trous non. Le trait
              va du centre d'une case au centre de l'autre ; passant pile par un coin, il
              file entre les deux cases qui le touchent. Survolez une case pour voir ce
              qu'elle verrait.</p>
          </details>
        </div>
      </div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);
  const outilActif = () =>
    (cible.querySelector('input[name=outilVue]:checked') || {}).value || 'depuis';
  const estCorps = (c) => vue.corps.some((k) => memeCase(k, c));
  const cachees = (r) => new Set([...((r && r.sol_cache) || []), ...((r && r.murs_caches) || [])].map(cle));

  function cliquer(x, y) {
    const ici = [x, y];
    message = '';
    if (outilActif() === 'depuis') {
      if (estCorps(ici)) message = ' Une créature se tient là : retirez-la pour y prendre place.';
      else vue.depuis = ici;
    } else if (memeCase(vue.depuis, ici)) {
      message = ' Vous vous tenez là : une créature se pose sur une autre case.';
    } else {
      const i = vue.corps.findIndex((k) => memeCase(k, ici));
      if (i >= 0) vue.corps.splice(i, 1);
      else vue.corps.push(ici);
    }
    interroger();
  }

  /// L'aperçu : l'ombre de la case survolée, le temps du survol. Seul l'outil
  /// « votre case » le montre, puisque c'est ce que son clic fixera.
  async function survoler(x, y) {
    demande += 1;
    const numero = demande;
    const ici = x == null ? null : [x, y];
    if (!ici || outilActif() !== 'depuis' || estCorps(ici) || memeCase(ici, vue.depuis)) {
      apercu = null;
      marquer($('plateauVue'), 'cachee', cachees(reponse));
      resumer();
      return;
    }
    try {
      const r = await appeler('vue', { ...pourLeServeur(plateau), depuis: ici, corps: vue.corps });
      if (numero !== demande) return;
      apercu = r;
      marquer($('plateauVue'), 'cachee', cachees(r));
      resumer();
    } catch (e) {
      // Un aperçu manqué ne mérite pas d'alerte : la case choisie reste juste.
    }
  }

  cible.querySelectorAll('input[name=outilVue]').forEach((radio) => {
    radio.addEventListener('change', () => {
      survoler(null, null);
      rafraichirApercu($('plateauVue'));
    });
  });

  /// Ce qu'un clic poserait sous le curseur : votre case, ou une créature en
  /// Poutch ; une croix si le clic la retirerait, un contour rouge si la case
  /// ne la prend pas.
  function aPoser(x, y) {
    const ici = [x, y];
    if (outilActif() === 'depuis') {
      if (estCorps(ici)) return interdit(ici);
      return memeCase(vue.depuis, ici) ? '' : fantome(jetonLettre(ici, 'lanceur', 'V'));
    }
    if (memeCase(vue.depuis, ici)) return interdit(ici);
    return estCorps(ici) ? retrait(ici) : fantome(poutchs([{ case: ici }]));
  }
  $('viderVue').addEventListener('click', () => {
    vue.corps = [];
    message = '';
    interroger();
  });
  $('choixPlateauVue').addEventListener('change', () => {
    choisir(etat, $('choixPlateauVue').value);
    plateau = plateauActif(etat);
    message = recalerVue();
    interroger();
  });

  /// Le compte des cases en vue, de la case choisie ou de la case survolée.
  function resumer() {
    const r = apercu || reponse;
    if (!r) { $('bilanVue').innerHTML = ''; return; }
    const cache = r.sol_cache.length;
    $('bilanVue').innerHTML = `<dl>
      <dt>${apercu ? 'Depuis la case survolée' : 'Depuis votre case'}</dt>
      <dd>${r.sol - cache} ${pluriel(r.sol - cache, 'case')} de sol en vue sur ${r.sol}</dd>
      <dt>Hors de vue</dt><dd>${cache} ${pluriel(cache, 'case')}</dd>
      <dt>Créatures posées</dt><dd>${vue.corps.length}</dd></dl>`;
  }

  function peindre() {
    const ombre = cachees(reponse);
    let jetons = '';
    const [vx, vy] = versEcran(vue.depuis[0], vue.depuis[1]);
    jetons += `<text class="jeton lanceur" x="${vx}" y="${vy}">V</text>`;
    jetons += poutchs(vue.corps.map((c) => ({ case: c })));
    dessiner($('plateauVue'), {
      plateau,
      case: (x, y) => {
        const ici = [x, y];
        const classes = [];
        if (ombre.has(cle(ici))) classes.push('cachee');
        if (memeCase(vue.depuis, ici)) classes.push('lanceur');
        if (estCorps(ici)) classes.push('creature');
        return {
          classes,
          titre: memeCase(vue.depuis, ici) ? 'Votre case'
            : estCorps(ici) ? 'Une créature'
            : ombre.has(cle(ici)) ? 'Hors de vue' : 'En vue',
        };
      },
      mur: (x, y) => ({ classes: ombre.has(cle([x, y])) ? ['cachee'] : [] }),
      jetons,
      surClic: cliquer,
      surSurvol: survoler,
      apercu: aPoser,
    });
    $('etatVue').textContent = (reponse
      ? 'En gris, ce que votre case ne voit pas.' : '') + message;
    resumer();
  }

  async function interroger() {
    apercu = null;
    demande += 1;
    try {
      reponse = await appeler('vue', { ...pourLeServeur(plateau), depuis: vue.depuis, corps: vue.corps });
    } catch (e) {
      reponse = null;
      message = ` ${e.message}`;
    }
    peindre();
  }

  peindre();
  await interroger();
}
