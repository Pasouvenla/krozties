// La barre du haut : le personnage, puis son build, en deux listes ; et
// « Enregistrer le build » quand le build chargé n'est à aucun personnage.

import { echapper } from './format.js';
import {
  ABREGES, CLASSES, ajouterBuild, chargerBuild, courant, creerPersonnage, ecrire, elementsDe, lire, personnage,
} from './personnages.js';

const nomDeClasse = (etat, id) => (((etat.classes && etat.classes.classes) || [])
  .find((c) => c.id === id) || {}).label || CLASSES[id] || '';

const badge = (p) => `<span class="badge-perso">${echapper(ABREGES[p.classe] || '?')}</span>`;

const nBuilds = (n) => `${n} build${n > 1 ? 's' : ''}`;

/// Dessine les listes dans `conteneur`, d'après les personnages enregistrés et
/// le build chargé. `outils` porte `appeler`, `majIdentite` et `afficher`.
export function dessinerSelecteur(conteneur, etat, outils) {
  const d = lire();
  const c = courant(d);
  const perso = c && c.perso;
  const libelle = (p) => `${echapper(p.nom)} · ${echapper(nomDeClasse(etat, p.classe))} ${p.niveau}`;
  const listes = d.personnages.length ? `
    <details class="choix" data-choix>
      <summary><span class="etiquette">Personnage</span>
        <span class="valeur">${perso ? libelle(perso) : 'aucun'}</span></summary>
      <div class="panneau-choix">
        ${d.personnages.map((p) => `
          <button type="button" class="option-perso${perso && p.id === perso.id ? ' choisi' : ''}" data-perso="${
            echapper(p.id)}"${p.builds.length ? '' : ' disabled'}>${badge(p)}<span><b>${echapper(p.nom)}</b>
            <small>${echapper(nomDeClasse(etat, p.classe))} ${p.niveau} · ${nBuilds(p.builds.length)}</small></span></button>`)
          .join('')}
        <button type="button" class="option-gerer" data-gerer>Gérer les personnages</button>
      </div>
    </details>
    <details class="choix" data-choix>
      <summary><span class="etiquette">Build</span>
        <span class="valeur">${c ? echapper(c.build.nom) : (etat.build ? 'non enregistré' : 'aucun')}</span></summary>
      <div class="panneau-choix">
        ${perso ? perso.builds.map((b) => `
          <button type="button" class="option-build${b.id === c.build.id ? ' choisi' : ''}" data-build="${
            echapper(b.id)}"><b>${echapper(b.nom)}</b><small>${echapper((b.elements || []).join(' · '))}</small></button>`)
          .join('') : '<p class="aide">Choisissez d\'abord un personnage.</p>'}
        <button type="button" class="option-gerer" data-gerer>Gérer les personnages</button>
      </div>
    </details>` : '';

  // Le build chargé à ranger : sous un personnage de sa classe, ou un nouveau.
  const r = etat.resolu;
  const compatibles = r ? d.personnages.filter((p) => p.classe === r.class) : [];
  const enregistrer = etat.build && r && !c ? `
    <details class="choix enregistrer" data-choix>
      <summary class="action discret">Enregistrer le build</summary>
      <div class="panneau-choix formulaire">
        <label for="nomBuildNeuf">Nom du build</label>
        <input type="text" id="nomBuildNeuf" value="${echapper(etat.nomDuBuild || '')}" placeholder="Terre Air PvM">
        <label for="persoDuBuild">Personnage</label>
        <select id="persoDuBuild">
          ${compatibles.map((p) => `<option value="${echapper(p.id)}">${echapper(p.nom)}</option>`).join('')}
          <option value="">Nouveau personnage ${echapper(nomDeClasse(etat, r.class))}</option>
        </select>
        <div id="champPersoNeuf"${compatibles.length ? ' hidden' : ''}>
          <label for="nomPersoNeuf">Nom du personnage</label>
          <input type="text" id="nomPersoNeuf" placeholder="Nom en jeu">
        </div>
        <button type="button" class="action" id="validerBuild">Enregistrer</button>
        <p class="aide" id="etatEnregistrement"></p>
      </div>
    </details>` : '';

  conteneur.innerHTML = `${listes}${enregistrer}<span class="alerte" id="etatSelecteur"></span>`;

  const q = (sel) => conteneur.querySelector(sel);
  const ouvertures = [...conteneur.querySelectorAll('[data-choix]')];
  ouvertures.forEach((o) => o.addEventListener('toggle', () => {
    if (o.open) ouvertures.filter((x) => x !== o).forEach((x) => { x.open = false; });
  }));

  // Charger un build ferme les listes et redessine l'outil ouvert.
  const charger = async (p, b) => {
    ouvertures.forEach((o) => { o.open = false; });
    try {
      await chargerBuild(etat, p, b, outils);
      await outils.afficher(etat.outil);
    } catch (e) {
      q('#etatSelecteur').textContent = e.message;
    }
  };
  conteneur.querySelectorAll('[data-perso]').forEach((bouton) => bouton.addEventListener('click', () => {
    const p = personnage(lire(), bouton.dataset.perso);
    const b = p && (p.builds.find((x) => x.id === p.dernier) || p.builds[0]);
    if (b) charger(p, b);
  }));
  conteneur.querySelectorAll('[data-build]').forEach((bouton) => bouton.addEventListener('click', () => {
    const p = perso && personnage(lire(), perso.id);
    const b = p && p.builds.find((x) => x.id === bouton.dataset.build);
    if (b) charger(p, b);
  }));
  conteneur.querySelectorAll('[data-gerer]').forEach((bouton) => bouton.addEventListener('click', () => {
    ouvertures.forEach((o) => { o.open = false; });
    outils.afficher('personnages');
  }));

  const choixPerso = q('#persoDuBuild');
  if (choixPerso) {
    choixPerso.addEventListener('change', () => { q('#champPersoNeuf').hidden = choixPerso.value !== ''; });
    q('#validerBuild').addEventListener('click', () => {
      const nom = q('#nomBuildNeuf').value.trim();
      const nomPerso = q('#nomPersoNeuf').value.trim();
      if (!nom) { q('#etatEnregistrement').textContent = 'Donnez un nom au build.'; return; }
      if (!choixPerso.value && !nomPerso) { q('#etatEnregistrement').textContent = 'Donnez un nom au personnage.'; return; }
      const donnees = lire();
      const p = choixPerso.value
        ? personnage(donnees, choixPerso.value)
        : creerPersonnage(donnees, { nom: nomPerso, classe: r.class, niveau: r.level });
      const b = ajouterBuild(p, {
        nom, lien: etat.lienDuBuild, charge: etat.build, dofusbook: etat.ficheDofusbook, elements: elementsDe(r),
      });
      donnees.courant = { perso: p.id, build: b.id };
      p.dernier = b.id;
      if (!ecrire(donnees)) { q('#etatEnregistrement').textContent = "Cet ordinateur refuse l'enregistrement."; return; }
      outils.majIdentite();
    });
  }
}

/// Une liste ouverte se ferme d'un clic ailleurs.
document.addEventListener('click', (e) => {
  document.querySelectorAll('#entete details[data-choix][open]').forEach((d) => {
    if (!d.contains(e.target)) d.open = false;
  });
});
