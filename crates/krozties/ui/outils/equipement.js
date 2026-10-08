// L'équipement : ce qu'on calcule, et la seule porte d'entrée. La barre du haut
// rappelle sur qui l'on travaille, et « Changer » y ramène.

import { decoderLienCourt } from '../commun/import-dofusbook.js';
import { COULEUR_ELEMENT, echapper, majuscule, nb } from '../commun/format.js';

/// Un lien, et rien d'autre : l'application lit l'équipement elle-même (voir
/// `pont.js`). Dans un navigateur, en développement, le pont refuse la lecture
/// avec une phrase qui le dit.
export async function rendre(cible, etat, { appeler, majIdentite, afficher }) {
  cible.innerHTML = `
    <section class="outil selectionnable">
      <h2>Équipement</h2>

      <div class="carte import">
        <h3>Importer un équipement</h3>
        <label for="lien">Lien DofusBook de l'équipement</label>
        <div class="saisie">
          <input type="url" id="lien" placeholder="https://d-bk.net/fr/d/XXXXX" spellcheck="false">
          <button type="button" class="action" id="ouvrir">Importer</button>
        </div>
        <p class="aide">L'équipement doit être public sur DofusBook.
          <span id="etatImport" role="status" aria-live="polite"></span></p>
      </div>

      <div id="detailBuild" style="margin-top:1rem"></div>
      <div id="classesConnues" style="margin-top:1rem"></div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);

  const importer = async () => {
    const id = decoderLienCourt($('lien').value);
    if (!id) {
      $('etatImport').innerHTML = '<span class="alerte">Lien non reconnu. '
        + 'Attendu : https://d-bk.net/fr/d/XXXXX</span>';
      return;
    }
    // Dire que ça travaille : la lecture attend DofusBook, et un bouton qui ne
    // réagit pas se reclique.
    $('ouvrir').disabled = true;
    $('etatImport').textContent = `Lecture de l'équipement n° ${id}…`;
    try {
      const r = await appeler('importer', { id });
      await adopter(r.build, r.dofusbook, r.nom);
    } catch (e) {
      $('etatImport').innerHTML = `<span class="alerte">${echapper(e.message)}</span>`;
    } finally {
      $('ouvrir').disabled = false;
    }
  };
  $('ouvrir').addEventListener('click', importer);
  // Entrée valide le lien : c'est le geste qu'on fait après un collage.
  $('lien').addEventListener('keydown', (e) => { if (e.key === 'Enter') importer(); });

  /// Ce qui suit la lecture est le MÊME chemin, que la charge vienne d'un
  /// collage ou de la lecture directe. Deux chemins de résolution séparés
  /// finiraient par diverger sur ce qu'ils acceptent.
  async function adopter(charge, dofusbook = null, nom = null) {
    $('etatImport').textContent = 'Résolution de l\'équipement…';
    try {
      // La fiche de DofusBook ne reste pas dans le build : elle ne sert qu'à
      // l'affichage, et le build part avec chaque calcul.
      const resolu = await appeler('resoudre', dofusbook ? { ...charge, dofusbook } : charge);
      if (resolu.error) throw new Error(resolu.error);
      etat.build = charge;
      etat.nomDuBuild = nom;
      etat.resolu = resolu;
      majIdentite();
      $('etatImport').textContent = '';
      detailler(resolu);
      listerLesClasses();
    } catch (e) {
      $('etatImport').innerHTML = `<span class="alerte">${echapper(e.message)}</span>`;
    }
  }

  /// La fiche du build importé, en trois colonnes : l'identité, les
  /// caractéristiques et la mobilité à gauche, l'équipement au centre, les dommages
  /// et résistances à droite, avec les chiffres de la fiche DofusBook lue à
  /// l'import. Les encarts et leurs libellés viennent du moteur.
  function detailler(r) {
    const db = r.fiche_dofusbook;
    const stuff = r.stuff || [];
    const parSlot = Object.fromEntries(stuff.map((e) => [e.slot, e]));
    const manquants = r.items_missing || 0;

    // L'infobulle d'une ligne : sa provenance, telle que DofusBook la détaille. Le
    // détail ne couvre pas les bonus actifs : l'écart avec le total est nommé.
    const provenance = (x) => {
      const lignes = (x.detail || []).map((d) => `${d.label} ${d.value > 0 ? '+' : ''}${nb(d.value)}`);
      const somme = (x.detail || []).reduce((t, d) => t + (Number(d.value) || 0), 0);
      const reste = (Number(x.value) || 0) - somme;
      if (Math.abs(reste) > 0.5) lignes.push(`Bonus actifs ${reste > 0 ? '+' : ''}${nb(reste)}`);
      return lignes.join('\n');
    };
    const stat = (x) => `<div class="stat"${x.detail ? ` title="${echapper(provenance(x))}"` : ''}>
      <b>${x.value == null ? '·' : nb(x.value)}</b><span>${echapper(x.label)}</span></div>`;
    const deuxColonnes = (colonnes) => `<div class="deux">${(colonnes || [])
      .map((c) => `<div>${c.map(stat).join('')}</div>`).join('')}</div>`;
    const encart = (titre, corps, classe = '') => `<section class="encart ${classe}">
      <h4>${echapper(titre)}</h4>${corps}</section>`;

    const caracteristiques = db ? `<table class="caracs">
        <thead><tr><th colspan="2"></th><th title="La caractéristique augmentée de la Puissance : c'est ce qu'elle pèse dans un sort">+ Puissance</th><th>Base</th><th>Parcho</th></tr></thead>
        <tbody>${db.caracteristiques.map((c) => `<tr title="${echapper(provenance(c))}">
          <td class="v">${nb(c.value)}</td><td>${echapper(c.label)}</td>
          <td class="v">${c.puissance == null ? '' : nb(c.puissance)}</td>
          <td class="v">${c.base == null ? '' : nb(c.base)}</td>
          <td class="v">${c.parcho == null ? '' : nb(c.parcho)}</td></tr>`).join('')}
          <tr class="pu" title="${echapper(provenance(db.puissance))}">
            <td class="v">${nb(db.puissance.value)}</td><td>${echapper(db.puissance.label)}</td>
            <td colspan="3"></td></tr>
        </tbody></table>` : '';

    // Le centre : l'équipement dans l'ordre de la fiche de DofusBook ; sans
    // personnage au milieu, les deux colonnes se rejoignent.
    const tuile = (slot) => {
      const e = parSlot[slot];
      if (!e) return '';
      if (!e.name) {
        return `<div class="tuile vide"><span class="objet vide"></span>
          <span class="nom"><small>${echapper(e.label)}</small>${e.inconnu ? 'objet inconnu' : 'vide'}</span></div>`;
      }
      // Au survol, la fiche de l'objet (`ficheDObjet`). Les écarts qui comptent pour
      // le calcul se voient en tête de fiche.
      return `<div class="tuile" data-objet="${echapper(slot)}">
        ${e.icon ? `<img class="objet" src="objets/${e.icon}.webp" alt="" decoding="async">`
          : '<span class="objet vide"></span>'}
        <span class="nom"><small>${echapper(e.label)} · niv. ${e.level || '?'}</small>${echapper(e.name)}
          ${(e.forgemagie || []).length ? `<em>${(e.forgemagie || [])
            .map((f) => `${echapper(f.label)} ${f.amount > 0 ? '+' : ''}${f.amount}${f.compte ? '' : ' (non compté)'}`)
            .join(' · ')}</em>` : ''}</span>
      </div>`;
    };
    const dofus = ['d1', 'd2', 'd3', 'd4', 'd5', 'd6'].map((slot) => {
      const e = parSlot[slot];
      if (!e || !e.name) return '<span class="trophee vide" title="Emplacement vide"></span>';
      return `<span class="trophee" data-objet="${echapper(slot)}">${
        e.icon ? `<img src="objets/${e.icon}.webp" alt="${echapper(e.name)}" decoding="async">` : ''}</span>`;
    }).join('');
    const poupee = `<div class="poupee">
        ${[['am', 'ch'], ['br', 'ar'], ['a1', 'a2'], ['ce', 'ca'], ['bo', 'fa'], ['mo']]
          .map((rang) => rang.map(tuile).join('')).join('')}
      </div>
      <div class="trophees">${dofus}</div>`;

    const exo = (r.forgemagie_totale || []).length
      ? `<div class="stats-liste">${r.forgemagie_totale.map((f) => `<div class="stat"><b>${nb(f.value)}</b><span>${
        echapper(f.label)}${f.compte ? '' : ' <i>non compté</i>'}</span></div>`).join('')}</div>`
      : '<p class="aide" style="margin:0">Aucune forgemagie.</p>';

    // Sans la fiche de DofusBook, les chiffres sont les nôtres, et ça se dit.
    const repli = db ? '' : `<p class="bandeau souci" style="margin:0 0 .8rem">La fiche de
      DofusBook n'a pas pu être lue : les chiffres ci-dessous sont ceux de Krozties.</p>`;
    const colonneGauche = db
      ? encart(etat.nomDuBuild || 'Personnage', deuxColonnes(db.identite))
        + encart('Caractéristiques', caracteristiques)
        + encart('Mobilité', deuxColonnes(db.mobilite))
      : encart('Caractéristiques', `<div class="stats-liste">${(r.totals || [])
        .map((x) => stat(x)).join('')}</div>`);
    const colonneDroite = db
      ? encart('Dommages', deuxColonnes(db.dommages))
        + encart('Résistances', deuxColonnes(db.resistances))
        + encart('Exo / Over des items', exo)
      : encart('Exo / Over des items', exo);

    // ⚠️ UN ÉCART ENTRE NOS CHIFFRES ET LES LEURS SE DIT SUR LA FICHE. Le moteur
    // calcule les dégâts sur son propre catalogue : si ce catalogue a vieilli
    // sur un objet de CE build, c'est ici que le joueur doit l'apprendre.
    const ecarts = (r.ecarts || []).length ? `<p class="bandeau souci" style="margin:0 0 .8rem">
      Krozties ne trouve pas les mêmes chiffres que DofusBook sur ${r.ecarts
        .map((x) => `${echapper(x.label)} (${nb(x.krozties)} au lieu de ${nb(x.dofusbook)})`).join(', ')} :
      les rotations sont calculées sur les chiffres de Krozties.</p>` : '';

    $('detailBuild').innerHTML = `
      <div class="carte fiche-carte">
        <h3>Fiche du build</h3>
        ${repli}${ecarts}
        ${manquants ? `<p class="bandeau souci" style="margin:0 0 .8rem">${manquants}
          objet${manquants > 1 ? 's' : ''} non reconnu${manquants > 1 ? 's' : ''} par Krozties :
          ${manquants > 1 ? 'leurs statistiques manquent' : 'ses statistiques manquent'} aux rotations.</p>` : ''}
        <div class="fiche-db">
          <div class="colonne">${colonneGauche}</div>
          <div class="colonne">${encart(`Niveau ${r.level || '?'} · ${r.class_name || 'classe inconnue'}`, poupee, 'centre')}</div>
          <div class="colonne">${colonneDroite}</div>
        </div>
        ${(r.assumptions || []).length || (r.uninterpreted || []).length ? `
          <details class="pourquoi" style="margin-top:.8rem">
            <summary>Ce qui a été supposé ou ignoré</summary>
            <ul class="aide" style="margin:.4rem 0 0">
              ${[...(r.uninterpreted || []), ...(r.assumptions || [])]
                .map((a) => `<li>${echapper(majuscule(a))}</li>`).join('')}
            </ul>
          </details>` : ''}
      </div>`;
  }

  /// Ce qui est modélisé se dit avant l'import, et disparaît une fois le build
  /// chargé. Une liste vide signalerait aussi que le moteur ne répond pas.
  function listerLesClasses() {
    const hote = $('classesConnues');
    if (etat.resolu) { hote.innerHTML = ''; return; }
    const c = etat.classes && etat.classes.classes;
    if (!c) {
      // La raison vient du pont : « le moteur n'a pas répondu » sans dire
      // pourquoi laisse chercher du côté du réseau une panne de lecture.
      const pourquoi = etat.refusCatalogue;
      hote.innerHTML = '<p class="bandeau souci">Le moteur n\'a pas répondu : '
        + "le catalogue des classes n'a pas pu être lu. Sans lui, aucun outil de "
        + `combat ne fonctionnera.${pourquoi ? ` ${echapper(pourquoi)}` : ''}</p>`;
      return;
    }
    // Deux pourcentages : `percent` (part des sorts de dégâts qui ont un modèle)
    // vaut 100 partout et ne dit rien ; `percent_complete`, celui de `dofus gaps`,
    // dit si les dégâts sont tous chiffrés.
    const finies = c.filter((x) => x.coverage && x.coverage.complete).length;
    hote.innerHTML = `<div class="carte">
      <h3>Classes modélisées · ${c.length}</h3>
      <p class="aide" style="margin:0 0 .5rem">Part des sorts de dégâts
        entièrement chiffrés. ${finies} ${finies > 1 ? 'classes sont complètes' : 'classe est complète'}.</p>
      <div class="classes">${c.map((x) => {
        const cov = x.coverage || {};
        return `<span class="classe${cov.complete ? ' complete' : ''}">${echapper(x.label)}${
          cov.percent_complete != null
            ? ` <span class="cout">${cov.percent_complete} %</span>` : ''}</span>`;
      }).join('')}</div>
    </div>`;
  }

  listerLesClasses();

  // Un équipement déjà chargé se réaffiche plutôt que de laisser un écran vide.
  if (etat.resolu) detailler(etat.resolu);

}
