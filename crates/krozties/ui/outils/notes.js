// Les notes de mise à jour : chaque version publiée, repliée sur une ligne ; la
// plus récente ouverte.

import { echapper } from '../commun/format.js';
import { changements, charger, dateLongue, rubriques } from '../commun/nouveautes.js';
import { hote } from '../pont.js';

export async function rendre(cible, etat, { appeler }) {
  const [versions, installee] = await Promise.all([
    charger(),
    hote === 'bureau' ? appeler('version').catch(() => null) : null,
  ]);
  cible.innerHTML = `
    <section class="outil notes-de-version">
      <h2>Notes de mise à jour</h2>
      ${installee ? `<p class="sous">Version installée : ${echapper(installee)}</p>` : ''}
      <div class="liste-versions">${versions.map((v, i) => `
        <details class="version"${i === 0 ? ' open' : ''}>
          <summary><b>Mise à jour ${echapper(v.version)}</b><small>${dateLongue(v.date)}</small>
            <span class="nb">${changements(v)}</span></summary>
          <div>${rubriques(v) || '<p class="aide">Aucun changement dans le jeu.</p>'}</div>
        </details>`).join('')}
      </div>
    </section>`;
}
