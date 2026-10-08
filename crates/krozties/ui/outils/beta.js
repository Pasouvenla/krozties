// Bêta en cours : les changements de sorts que la note de la bêta annonce,
// classe par classe, avant leur sortie. La donnée vient de `dofus beta`, qui lit
// les notes de la bêta à blanc : rien de la bêta n'entre dans le calcul. Chaque
// changement se montre au plus haut grade.

import { echapper } from '../commun/format.js';

/// Une ligne : un changement, son libellé puis « de → vers », ou une phrase.
function ligne(l) {
  if (!l.libelle) return `<p class="aide">${echapper(l.texte)}</p>`;
  const valeurs = l.de ? `${echapper(l.de)} → ${echapper(l.vers)}` : echapper(l.vers);
  return `<div class="beta-ligne"><span class="libelle">${echapper(l.libelle)}</span>`
    + `<span>${valeurs}</span></div>`;
}

export async function rendre(cible, etat, { appeler }) {
  const d = await appeler('beta');
  const classes = d.classes || [];
  if (!classes.length) {
    cible.innerHTML = `<section class="outil"><h2>Bêta en cours</h2>
      <p class="sous">Aucune bêta en cours.</p></section>`;
    return;
  }
  // La classe du personnage d'abord, puis l'ordre de la note.
  const notre = etat.resolu && etat.resolu.class;
  const rangees = [...classes].sort((a, b) => (b.classe === notre) - (a.classe === notre));
  let choisie = rangees.some((c) => c.classe === etat.betaClasse) ? etat.betaClasse : rangees[0].classe;
  const titre = (d.notes && d.notes[0] && d.notes[0].titre) || '';
  const date = (titre.match(/\d{2}\/\d{2}\/\d{4}/) || [])[0];

  const dessiner = () => {
    const c = rangees.find((x) => x.classe === choisie);
    cible.innerHTML = `
      <section class="outil beta">
        <h2>Bêta en cours</h2>
        <p class="sous">Bêta ${echapper(d.version || '')}${date ? ` · note du ${date}` : ''}</p>
        <div class="barre-outils" role="group" aria-label="Classes">${rangees.map((x) => `
          <button type="button" class="etape" data-classe="${x.classe}"
            aria-pressed="${x.classe === choisie}">${echapper(x.nom)} ${x.changements}</button>`).join('')}
        </div>
        <div class="beta-classe">
          ${c.generales.map((t) => `<p class="aide">${echapper(t)}</p>`).join('')}
          ${c.sorts.map((s) => `
            <div class="beta-sort">
              <h4${s.ids.length === 1 ? ` data-sort="${echapper(s.ids[0])}"` : ''}>${echapper(s.nom)}</h4>
              ${s.lignes.map(ligne).join('')}
            </div>`).join('')}
        </div>
      </section>`;
    cible.querySelectorAll('[data-classe]').forEach((b) => b.addEventListener('click', () => {
      choisie = Number(b.dataset.classe);
      etat.betaClasse = choisie;
      dessiner();
    }));
  };
  dessiner();
}
