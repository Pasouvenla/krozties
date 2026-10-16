// Données manquantes : ce que la rotation ne compte pas encore, classe par
// classe, et les objets qu'une note change sans donner leurs chiffres. Chaque
// point se relève en jeu, puis part sur GitHub dans un formulaire déjà rempli.

import { hote } from '../pont.js';
import { echapper } from '../commun/format.js';

/// Le formulaire d'issue des données manquantes, rempli par son adresse.
const FORMULAIRE = 'https://github.com/Pasouvenla/krozties/issues/new?template=donnee-manquante.yml';

const GENRES = { valeur: 'Valeur', effet: 'Effet', etat: 'État', objet: 'Objet' };

const EXEMPLES = {
  valeur: 'ex. 2 lancers par tour',
  effet: 'ex. +10 % de dommages en mêlée, 2 tours',
  etat: 'ex. +20 % de dommages pour Carnavalo',
  objet: 'ex. 80 à 100 Force, 300 Vitalité',
};

/// Les onglets : la classe du personnage d'abord, puis celles qui ont le plus
/// de points ; les objets en tête, quand une note en laisse.
function onglets(d, notre) {
  const classes = d.classes.filter((c) => c.points > 0)
    .sort((a, b) => (b.id === notre) - (a.id === notre) || b.points - a.points);
  const liste = classes.map((c) => ({ cle: `classe-${c.id}`, nom: c.nom, points: c.points, classe: c }));
  if (d.objets.length) {
    liste.unshift({
      cle: 'objets',
      nom: 'Objets',
      points: d.objets.length,
      objets: d.objets.map((o) => ({
        nom: o.nom,
        points: [{
          genre: 'objet',
          texte: `Ses caractéristiques depuis la ${o.note.replace(/^Mise à jour /, '')}`,
          aide: 'La note change cet objet sans donner ses chiffres. Relevez-les en jeu.',
          reference: `objet ${o.id}`,
        }],
      })),
    });
  }
  return liste;
}

function formulaire(cle, genre, niveau) {
  return `
    <div class="formulaire-point" id="form-${cle}" hidden>
      <div>
        <label for="vu-${cle}">Ce que vous voyez en jeu</label>
        ${genre === 'objet'
    ? `<textarea id="vu-${cle}" rows="3" placeholder="${EXEMPLES[genre]}"></textarea>`
    : `<input type="text" id="vu-${cle}" placeholder="${EXEMPLES[genre]}">`}
      </div>
      ${genre === 'objet' ? '' : `<div>
        <label for="niveau-${cle}">Niveau du personnage</label>
        <input type="number" id="niveau-${cle}" min="1" max="200" value="${niveau}">
      </div>`}
      <div class="large">
        <label for="precisions-${cle}">Précisions (facultatif)</label>
        <textarea id="precisions-${cle}" rows="2"
          placeholder="Une condition, un cumul, ce qui change en coup critique…"></textarea>
      </div>
      <div class="pied">
        <button type="button" class="action ouvrir" data-envoyer="${cle}">Ouvrir sur GitHub</button>
        <p class="aide" id="etat-${cle}">Un compte GitHub est nécessaire.</p>
      </div>
    </div>`;
}

function carte(c, i, niveau, points) {
  const icone = c.icone ? `<img src="icons/${c.icone}.png" alt="" decoding="async" loading="lazy">` : '';
  return `
    <article class="carte-manquante">
      <header class="dm-sort-nom">${icone}<span${c.sort ? ` data-sort="${echapper(c.sort)}"` : ''}>${echapper(c.nom)}</span></header>
      ${c.points.map((p, j) => {
    const cle = `${i}-${j}`;
    points.set(cle, { carte: c, point: p });
    return `
        <div class="point-manquant">
          <span class="dm-type">${GENRES[p.genre]}</span>
          <div>
            <p class="releve">${echapper(p.texte)}</p>
            <p class="aide">${echapper(p.aide)}</p>
          </div>
          <button type="button" class="action discret petit" data-repondre="${cle}"
            aria-expanded="false" aria-controls="form-${cle}">Répondre</button>
          ${formulaire(cle, p.genre, niveau)}
        </div>`;
  }).join('')}
    </article>`;
}

export async function rendre(cible, etat, { appeler }) {
  let d;
  try {
    d = await appeler('manquantes');
  } catch (e) {
    cible.innerHTML = `<section class="outil"><h2>Données manquantes</h2>
      <p class="sous alerte">La liste n'a pas pu se charger : ${echapper(e.message)}</p></section>`;
    return;
  }
  const version = hote === 'bureau' ? await appeler('version').catch(() => null) : null;
  const notre = etat.resolu && etat.resolu.class;
  const liste = onglets(d, notre);
  if (!liste.length) {
    cible.innerHTML = `<section class="outil"><h2>Données manquantes</h2>
      <p class="sous">Rien à relever pour l'instant.</p></section>`;
    return;
  }
  const niveau = (etat.resolu && etat.resolu.level) || 200;
  const sansPoint = d.classes.find((c) => c.id === notre && c.points === 0);
  let choisi = liste.some((o) => o.cle === etat.manquantes) ? etat.manquantes : liste[0].cle;

  const dessiner = () => {
    const o = liste.find((x) => x.cle === choisi);
    const points = new Map();
    const cartes = o.objets ? o.objets : o.classe.cartes;
    const hors = (o.classe && o.classe.hors) || [];
    const nbHors = hors.reduce((n, h) => n + h.sorts.length, 0);
    cible.innerHTML = `
      <section class="outil manquantes">
        <h2>Données manquantes</h2>
        <p class="sous">Vérifiez un de ces points en jeu, puis envoyez ce que vous voyez.</p>
        <div class="barre-outils" role="group" aria-label="Classes">${liste.map((x) => `
          <button type="button" class="etape" data-onglet="${x.cle}"
            aria-pressed="${x.cle === choisi}">${echapper(x.nom)} <span class="compte">${x.points}</span></button>`).join('')}
        </div>
        ${sansPoint ? '<p class="aide">Rien à relever pour votre classe.</p>' : ''}
        <div class="liste-manquante">
          ${cartes.map((c, i) => carte(c, i, niveau, points)).join('')}
        </div>
        ${nbHors ? `<details class="hors-calcul">
          <summary>${nbHors} ${nbHors > 1 ? 'points hors du calcul' : 'point hors du calcul'}</summary>
          <dl>${hors.map((h) => `<dt>${echapper(h.raison)}</dt><dd>${h.sorts.map(echapper).join(', ')}</dd>`).join('')}</dl>
        </details>` : ''}
      </section>`;

    cible.querySelectorAll('[data-onglet]').forEach((b) => b.addEventListener('click', () => {
      choisi = b.dataset.onglet;
      etat.manquantes = choisi;
      dessiner();
    }));
    cible.querySelectorAll('[data-repondre]').forEach((b) => b.addEventListener('click', () => {
      const form = cible.querySelector(`#form-${b.dataset.repondre}`);
      form.hidden = !form.hidden;
      b.setAttribute('aria-expanded', String(!form.hidden));
      if (!form.hidden) form.querySelector('input, textarea').focus();
    }));
    cible.querySelectorAll('[data-envoyer]').forEach((b) => b.addEventListener('click', async () => {
      const cle = b.dataset.envoyer;
      const { carte: c, point: p } = points.get(cle);
      const champ = (nom) => {
        const e = cible.querySelector(`#${nom}-${cle}`);
        return e ? e.value.trim() : '';
      };
      const vu = champ('vu');
      const statut = cible.querySelector(`#etat-${cle}`);
      if (!vu) {
        statut.innerHTML = '<span class="alerte">Tapez d\'abord ce que vous voyez en jeu.</span>';
        cible.querySelector(`#vu-${cle}`).focus();
        return;
      }
      const classe = o.classe ? o.classe.nom : '';
      const champs = {
        title: `Donnée : ${classe ? `${classe} · ` : ''}${c.nom}`,
        classe,
        sort: c.nom,
        point: `${p.texte}\n${p.aide}`,
        observation: vu,
        niveau: champ('niveau'),
        precisions: champ('precisions'),
        versions: `Dofus ${d.version_du_jeu} · ${version ? `Krozties ${version}` : 'Krozties (navigateur)'}`,
        reference: p.reference,
      };
      const adresse = FORMULAIRE + Object.entries(champs).filter(([, v]) => v)
        .map(([k, v]) => `&${k}=${encodeURIComponent(v)}`).join('');
      try {
        if (hote === 'bureau') await appeler('retour', { adresse });
        else window.open(adresse, '_blank', 'noopener');
        statut.textContent = 'Formulaire ouvert dans le navigateur : vérifiez-le, puis envoyez-le sur GitHub.';
      } catch (e) {
        statut.innerHTML = `<span class="alerte">${echapper(e.message)}</span>`;
      }
    }));
  };
  dessiner();
}
