// Gestion des personnages : les créer, les modifier, les supprimer, et
// importer, charger, renommer ou supprimer leurs builds. Tout reste sur cet
// ordinateur ; la liste s'exporte en un fichier, à garder ou à ouvrir ailleurs.

import { hote } from '../pont.js';
import { COULEUR_ELEMENT, echapper, majuscule } from '../commun/format.js';
import { decoderLienCourt } from '../commun/import-dofusbook.js';
import * as P from '../commun/personnages.js';

export async function rendre(cible, etat, { appeler, majIdentite }) {
  const classes = (etat.classes && etat.classes.classes) || [];
  const nomDeClasse = (id) => (classes.find((c) => c.id === id) || {}).label || P.CLASSES[id] || '?';
  // L'état de la page : la carte en édition (`nouveau` à la création), le
  // build qu'on renomme, la suppression qui attend son second clic.
  let edition = null;
  let renomme = null;
  let aConfirmer = null;
  let minuterie = null;
  let message = '';

  const puces = (b) => (b.elements || []).map((e) => `<span class="puce-element" style="background:${
    COULEUR_ELEMENT[e] || 'var(--doux)'}">${echapper(e)}</span>`).join('');

  /// « le 15/10 à 21:40 », à l'heure de cet ordinateur.
  const lu = (iso) => {
    const d = new Date(iso);
    if (!iso || Number.isNaN(d.getTime())) return '';
    const deux = (n) => String(n).padStart(2, '0');
    return `le ${deux(d.getDate())}/${deux(d.getMonth() + 1)} à ${deux(d.getHours())}:${deux(d.getMinutes())}`;
  };

  const supprimer = (cle, texte) => `<button type="button" class="action discret petit supprimer${
    aConfirmer === cle ? ' confirmer' : ''}" data-supprimer="${echapper(cle)}">${
    aConfirmer === cle ? 'Confirmer la suppression' : texte}</button>`;

  function ligneBuild(p, b, c) {
    const charge = c && c.build.id === b.id;
    const nom = renomme === b.id
      ? `<input type="text" class="renommer" value="${echapper(b.nom)}" data-renommer="${echapper(b.id)}"
          aria-label="Nouveau nom du build">`
      : `<b>${echapper(b.nom)}</b>`;
    return `<div class="build-perso${charge ? ' charge' : ''}">
      <div class="quoi">${nom}<small>${puces(b)}${b.lien ? `<span class="lien">${echapper(b.lien)}</span>` : ''}${
        b.lu_le ? `<span class="date-import">lu sur DofusBook ${lu(b.lu_le)}</span>` : ''}</small></div>
      <div class="actions">
        ${charge ? '<span class="etat-charge">Chargé</span>'
          : `<button type="button" class="action discret petit" data-charger="${echapper(b.id)}">Charger</button>`}
        ${hote === 'bureau' && b.lien
          ? `<button type="button" class="action discret petit" data-maj="${echapper(b.id)}">Mettre à jour</button>` : ''}
        ${renomme === b.id
          ? `<button type="button" class="action petit" data-valider-nom="${echapper(b.id)}">Valider</button>`
          : `<button type="button" class="action discret petit" data-renommer-build="${echapper(b.id)}">Renommer</button>`}
        ${supprimer(`build:${p.id}:${b.id}`, 'Supprimer')}
      </div></div>`;
  }

  function champs(p) {
    const classe = p ? p.classe : (etat.resolu && etat.resolu.class) || (classes[0] && classes[0].id);
    const niveau = p ? p.niveau : (etat.resolu && etat.resolu.level) || 200;
    const figee = p && p.builds.length > 0;
    return `<div class="champs-perso">
      <label>Nom <input type="text" id="champNom" value="${echapper(p ? p.nom : '')}" placeholder="Nom en jeu"></label>
      <label>Classe <select id="champClasse"${figee ? ' disabled' : ''}>${classes.map((k) => `<option value="${k.id}"${
        k.id === classe ? ' selected' : ''}>${echapper(k.label)}</option>`).join('')}</select></label>
      <label>Niveau <input type="number" id="champNiveau" value="${niveau}" min="1" max="200"></label>
    </div>${figee ? '<p class="aide">La classe suit les builds : supprimez-les pour la changer.</p>' : ''}`;
  }

  function carte(p, c) {
    const enEdition = edition === (p ? p.id : 'nouveau');
    const actuel = c && p && c.perso.id === p.id;
    const titre = p ? `${echapper(p.nom)} <span class="rang">· ${echapper(nomDeClasse(p.classe))} ${p.niveau}</span>`
      : 'Nouveau personnage';
    const boutons = enEdition
      ? `<button type="button" class="action petit" data-valider-perso>Enregistrer</button>
         <button type="button" class="action discret petit" data-annuler>Annuler</button>`
      : `<button type="button" class="action discret petit" data-modifier="${echapper(p.id)}">Modifier</button>
         ${supprimer(`perso:${p.id}`, 'Supprimer')}`;
    const builds = p ? (p.builds.map((b) => ligneBuild(p, b, c)).join('')
      || '<p class="aide">Aucun build : importez-en un depuis DofusBook.</p>') : '';
    const champImport = p ? `<div class="import-build">
        <input type="url" placeholder="https://d-bk.net/fr/d/XXXXX" data-lien="${echapper(p.id)}"
          aria-label="Lien DofusBook du build pour ${echapper(p.nom)}">
        <button type="button" class="action" data-importer="${echapper(p.id)}">Importer un build pour ${
          echapper(p.nom)}</button></div>` : '';
    return `<section class="encart carte-perso${actuel ? ' actuel' : ''}">
      <h4><span class="badge-perso">${echapper(P.ABREGES[p ? p.classe : 0] || '+')}</span><span>${titre}</span>
        <span class="espace"></span>${boutons}</h4>
      <div>${enEdition ? champs(p) : ''}${builds}${champImport}</div></section>`;
  }

  function dessiner() {
    const d = P.lire();
    const c = P.courant(d);
    const cartes = d.personnages.map((p) => carte(p, c)).join('');
    cible.innerHTML = `
      <section class="outil personnages">
        <h2>Gestion des personnages</h2>
        <p class="sous">Créez vos personnages, puis rangez-y vos builds DofusBook.</p>
        <div class="barre-page">
          <button type="button" class="action" id="creerPerso">+ Créer un personnage</button>
          <span class="espace"></span>
          <button type="button" class="action discret petit" id="exporterListe"${
            d.personnages.length ? '' : ' disabled'}>Exporter la liste</button>
          <label class="action discret petit fichier">Importer une liste
            <input type="file" accept=".json,application/json" id="importerListe"></label>
        </div>
        <p class="aide" id="etatPersos">${message
          ? echapper(message) : 'Exporter donne un fichier à garder ou à ouvrir sur un autre ordinateur.'}</p>
        <div class="pile-encarts">
          ${edition === 'nouveau' ? carte(null, c) : ''}
          ${cartes || (edition === 'nouveau' ? '' : '<p class="aide">Aucun personnage pour l\'instant.</p>')}
        </div>
      </section>`;
    brancher();
  }

  const redessiner = (texte = '') => {
    message = majuscule(texte);
    dessiner();
  };

  function brancher() {
    const $ = (sel) => cible.querySelector(sel);
    const tous = (sel) => [...cible.querySelectorAll(sel)];
    $('#creerPerso').addEventListener('click', () => { edition = 'nouveau'; redessiner(); });
    tous('[data-modifier]').forEach((b) => b.addEventListener('click', () => {
      edition = b.dataset.modifier;
      redessiner();
    }));
    tous('[data-annuler]').forEach((b) => b.addEventListener('click', () => { edition = null; redessiner(); }));
    tous('[data-valider-perso]').forEach((b) => b.addEventListener('click', () => {
      const nom = $('#champNom').value.trim();
      const niveau = Number($('#champNiveau').value);
      if (!nom) { redessiner('Donnez un nom au personnage.'); return; }
      if (!Number.isInteger(niveau) || niveau < 1 || niveau > 200) {
        redessiner('Le niveau va de 1 à 200.');
        return;
      }
      const d = P.lire();
      if (edition === 'nouveau') {
        P.creerPersonnage(d, { nom, classe: Number($('#champClasse').value), niveau });
      } else {
        const p = P.personnage(d, edition);
        Object.assign(p, { nom, niveau });
        if (!p.builds.length) p.classe = Number($('#champClasse').value);
      }
      if (!P.ecrire(d)) { redessiner("Cet ordinateur refuse l'enregistrement."); return; }
      edition = null;
      majIdentite();
      redessiner();
    }));

    // Une suppression attend un second clic, cinq secondes au plus.
    tous('[data-supprimer]').forEach((b) => b.addEventListener('click', () => {
      const cle = b.dataset.supprimer;
      if (aConfirmer !== cle) {
        aConfirmer = cle;
        clearTimeout(minuterie);
        minuterie = setTimeout(() => { aConfirmer = null; dessiner(); }, 5000);
        dessiner();
        return;
      }
      aConfirmer = null;
      clearTimeout(minuterie);
      const [genre, persoId, buildId] = cle.split(':');
      const d = P.lire();
      if (genre === 'perso') {
        d.personnages = d.personnages.filter((p) => p.id !== persoId);
        if (d.courant && d.courant.perso === persoId) d.courant = null;
      } else {
        const p = P.personnage(d, persoId);
        if (p) p.builds = p.builds.filter((x) => x.id !== buildId);
        if (d.courant && d.courant.build === buildId) d.courant = null;
      }
      P.ecrire(d);
      majIdentite();
      redessiner(genre === 'perso' ? 'Personnage supprimé.' : 'Build supprimé.');
    }));

    tous('[data-renommer-build]').forEach((b) => b.addEventListener('click', () => {
      renomme = b.dataset.renommerBuild;
      redessiner();
      const champ = cible.querySelector(`[data-renommer="${CSS.escape(renomme)}"]`);
      if (champ) { champ.focus(); champ.select(); }
    }));
    const validerNom = (id) => {
      const champ = cible.querySelector(`[data-renommer="${CSS.escape(id)}"]`);
      const nom = champ && champ.value.trim();
      if (!nom) return;
      const d = P.lire();
      for (const p of d.personnages) for (const b of p.builds) if (b.id === id) b.nom = nom;
      P.ecrire(d);
      if (etat.build && d.courant && d.courant.build === id) etat.nomDuBuild = nom;
      renomme = null;
      majIdentite();
      redessiner();
    };
    tous('[data-valider-nom]').forEach((b) => b.addEventListener('click', () => validerNom(b.dataset.validerNom)));
    tous('[data-renommer]').forEach((champ) => champ.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') validerNom(champ.dataset.renommer);
      if (e.key === 'Escape') { renomme = null; redessiner(); }
    }));

    tous('[data-charger]').forEach((b) => b.addEventListener('click', async () => {
      const d = P.lire();
      const p = d.personnages.find((x) => x.builds.some((y) => y.id === b.dataset.charger));
      try {
        await P.chargerBuild(etat, p, p.builds.find((y) => y.id === b.dataset.charger), { appeler, majIdentite });
        redessiner();
      } catch (e) {
        redessiner(e.message);
      }
    }));

    tous('[data-maj]').forEach((b) => b.addEventListener('click', () => mettreAJour(b.dataset.maj)));
    tous('[data-importer]').forEach((b) => b.addEventListener('click', () => importer(b.dataset.importer)));
    tous('[data-lien]').forEach((champ) => champ.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') importer(champ.dataset.lien);
    }));

    $('#exporterListe').addEventListener('click', exporterListe);
    $('#importerListe').addEventListener('change', (e) => importerListe(e.target.files[0]));
  }

  async function importer(persoId) {
    const champ = cible.querySelector(`[data-lien="${CSS.escape(persoId)}"]`);
    const lien = champ ? champ.value.trim() : '';
    const id = decoderLienCourt(lien);
    if (!id) { redessiner('Lien non reconnu. Attendu : https://d-bk.net/fr/d/XXXXX'); return; }
    message = `Lecture de l'équipement n° ${id}…`;
    cible.querySelector('#etatPersos').textContent = message;
    try {
      const r = await appeler('importer', { id });
      const d = P.lire();
      const p = P.personnage(d, persoId);
      if (r.build.class !== p.classe) {
        redessiner(`Ce build est ${nomDeClasse(r.build.class)} : importez-le sur un personnage de cette classe.`);
        return;
      }
      const resolu = await appeler('resoudre', r.dofusbook ? { ...r.build, dofusbook: r.dofusbook } : r.build);
      if (resolu.error) throw new Error(resolu.error);
      const b = P.ajouterBuild(p, {
        nom: r.nom || `Build ${p.builds.length + 1}`, lien, charge: r.build, dofusbook: r.dofusbook,
        elements: P.elementsDe(resolu), lu_le: new Date().toISOString(),
      });
      if (!P.ecrire(d)) { redessiner("Cet ordinateur refuse l'enregistrement."); return; }
      await P.chargerBuild(etat, p, b, { appeler, majIdentite });
      redessiner(`« ${b.nom} » importé et chargé.`);
    } catch (e) {
      redessiner(e.message);
    }
  }

  /// Relit un build sur DofusBook par son lien : son équipement change, son
  /// nom reste celui du joueur ; chargé, il se recharge aussitôt.
  async function mettreAJour(buildId) {
    const d = P.lire();
    const p = d.personnages.find((x) => x.builds.some((y) => y.id === buildId));
    const b = p && p.builds.find((y) => y.id === buildId);
    const id = b && decoderLienCourt(b.lien || '');
    if (!id) { redessiner('Lien du build non reconnu.'); return; }
    message = `Lecture de « ${b.nom} » sur DofusBook…`;
    cible.querySelector('#etatPersos').textContent = message;
    try {
      const r = await appeler('importer', { id });
      if (r.build.class !== p.classe) {
        redessiner(`Ce build est devenu ${nomDeClasse(r.build.class)} sur DofusBook : importez-le sur un personnage de cette classe.`);
        return;
      }
      const resolu = await appeler('resoudre', r.dofusbook ? { ...r.build, dofusbook: r.dofusbook } : r.build);
      if (resolu.error) throw new Error(resolu.error);
      Object.assign(b, {
        charge: r.build, dofusbook: r.dofusbook, elements: P.elementsDe(resolu), lu_le: new Date().toISOString(),
      });
      if (!P.ecrire(d)) { redessiner("Cet ordinateur refuse l'enregistrement."); return; }
      const c = P.courant(d);
      if (c && c.build.id === b.id) await P.chargerBuild(etat, p, b, { appeler, majIdentite });
      redessiner(`« ${b.nom} » mis à jour depuis DofusBook.`);
    } catch (e) {
      redessiner(e.message);
    }
  }

  async function exporterListe() {
    const contenu = P.exporter(P.lire());
    try {
      if (hote === 'bureau') {
        const chemin = await appeler('exporter', { contenu });
        redessiner(chemin ? `Liste exportée : ${chemin}` : '');
      } else {
        const a = document.createElement('a');
        a.href = URL.createObjectURL(new Blob([contenu], { type: 'application/json' }));
        a.download = 'krozties-personnages.json';
        a.click();
        URL.revokeObjectURL(a.href);
        redessiner('Liste exportée.');
      }
    } catch (e) {
      redessiner(e.message);
    }
  }

  async function importerListe(fichier) {
    if (!fichier) return;
    try {
      const d = P.lire();
      const n = P.importer(d, await fichier.text());
      if (!P.ecrire(d)) { redessiner("Cet ordinateur refuse l'enregistrement."); return; }
      majIdentite();
      redessiner(`${n} personnage${n > 1 ? 's' : ''} importé${n > 1 ? 's' : ''}.`);
    } catch (e) {
      redessiner(e instanceof SyntaxError ? 'Ce fichier ne se lit pas : est-ce bien une liste exportée ?' : e.message);
    }
  }

  dessiner();
}
