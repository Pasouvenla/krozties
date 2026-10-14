// Retours : un calcul faux, un souci d'affichage, une idée. Le joueur décrit,
// puis ouvre le formulaire d'issue du dépôt déjà rempli, où il colle sa
// capture : l'API de GitHub ne joint pas d'image à une issue.

import { appeler, hote } from '../pont.js';
import { echapper } from '../commun/format.js';

/// Le formulaire d'issue du dépôt, rempli par son adresse.
const FORMULAIRE = 'https://github.com/Pasouvenla/krozties/issues/new';

const SUJETS = [['calcul', 'Calcul faux'], ['affichage', 'Affichage'], ['idee', 'Idée']];

const POSITIONS = { libre: 'au choix', distance: 'à distance', melee: 'en mêlée' };

/// Ce qui part avec le retour : de quoi refaire le calcul.
function contexte(etat, version) {
  const r = etat.resolu;
  const reglages = etat.rotation;
  return [
    version ? `Krozties ${version}` : 'Krozties (navigateur)',
    r ? `${r.class_name || 'Classe inconnue'} niveau ${r.level || '?'}` : 'Aucun équipement chargé',
    etat.lienDuBuild ? `Build : ${etat.lienDuBuild}` : '',
    reglages ? `Rotation : ${reglages.horizon} tours, ${reglages.cibles} ennemi${reglages.cibles > 1 ? 's' : ''}, `
      + `position ${POSITIONS[reglages.position] || POSITIONS.libre}` : '',
  ].filter(Boolean);
}

export async function rendre(cible, etat) {
  const version = hote === 'bureau' ? await appeler('version').catch(() => null) : null;
  const joint = contexte(etat, version);
  const brouillon = etat.retour || (etat.retour = { sujet: 'calcul', description: '' });
  cible.innerHTML = `
    <section class="outil retours">
      <h2>Retours</h2>
      <p class="sous">Signalez un calcul faux, un souci d'affichage ou proposez une idée.</p>
      <div class="carte">
        <fieldset class="sujet"><legend>Sujet</legend>
          ${SUJETS.map(([v, t]) => `<label class="segment"><input type="radio" name="sujetRetour" value="${v}"${
            brouillon.sujet === v ? ' checked' : ''}> ${t}</label>`).join('')}
        </fieldset>
        <label for="descriptionRetour">Description</label>
        <textarea id="descriptionRetour" rows="5"
          placeholder="Ce que vous attendiez, ce que Krozties affiche, et sur quel sort.">${
            echapper(brouillon.description)}</textarea>
        <p class="aide">Joint au formulaire : ${joint.map(echapper).join(' · ')}.</p>
        <button type="button" class="action ouvrir" id="ouvrirRetour">Ouvrir le formulaire GitHub</button>
        <p class="aide" id="etatRetour">Collez votre capture d'écran dans le formulaire (Ctrl+V).
          Un compte GitHub est nécessaire.</p>
      </div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);
  cible.querySelectorAll('input[name=sujetRetour]').forEach((i) => {
    i.addEventListener('change', () => { brouillon.sujet = i.value; });
  });
  $('descriptionRetour').addEventListener('input', (e) => { brouillon.description = e.target.value; });

  $('ouvrirRetour').addEventListener('click', async () => {
    const sujet = (SUJETS.find(([v]) => v === brouillon.sujet) || SUJETS[0])[1];
    const description = brouillon.description.trim();
    const resume = description.split('\n')[0].slice(0, 70);
    const titre = resume ? `${sujet} : ${resume}` : sujet;
    const corps = [
      description || '(à décrire)',
      '',
      '---',
      `- Sujet : ${sujet}`,
      ...joint.map((j) => `- ${j}`),
      '',
      "Capture d'écran : collez-la ici (Ctrl+V).",
    ].join('\n');
    const adresse = `${FORMULAIRE}?title=${encodeURIComponent(titre)}&body=${encodeURIComponent(corps)}`;
    try {
      if (hote === 'bureau') await appeler('retour', { adresse });
      else window.open(adresse, '_blank', 'noopener');
      $('etatRetour').textContent = 'Formulaire ouvert dans le navigateur. Collez-y votre capture (Ctrl+V).';
    } catch (e) {
      $('etatRetour').innerHTML = `<span class="alerte">${echapper(e.message)}</span>`;
    }
  });
}
