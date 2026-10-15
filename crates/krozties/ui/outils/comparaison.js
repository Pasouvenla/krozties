// Comparaison : deux builds enregistrés d'une même classe, calculés avec les
// réglages de la Rotation, côte à côte.

import { cochesParDefaut, MAX_SORTS, sortsDuBuild } from '../commun/deck.js';
import { echapper, nb, pluriel } from '../commun/format.js';
import * as discord from '../commun/discord.js';
import { deuxBuilds } from '../commun/graphique.js';
import { CLASSES, courant, lire } from '../commun/personnages.js';
import { reglagesRotation } from '../commun/reglages-rotation.js';
import { decrire, versMoteur } from '../commun/resistances.js';

const ROUBLARD = 13;
const CHARGE = '__charge';
const POSITIONS = { libre: 'au choix', distance: 'à distance', melee: 'en mêlée' };

/// L'écart de B sur A, en pourcentage signé.
function ecart(a, b) {
  if (!a) return { texte: '', classe: '' };
  const p = ((b - a) / a) * 100;
  const v = Math.abs(p).toFixed(1).replace('.', ',');
  if (v === '0,0') return { texte: '0 %', classe: '' };
  return { texte: `${p < 0 ? '−' : '+'}${v} %`, classe: p < 0 ? 'moins' : 'plus' };
}

/// Combien de fois chaque sort est lancé dans une rotation.
function lancers(r) {
  const n = new Map();
  r.turns.forEach((t) => t.casts.forEach((c) => n.set(c.id, (n.get(c.id) || 0) + 1)));
  return n;
}

/// L'écart de B sur A, en points de dégâts signés.
function ecartEnPoints(a, b) {
  const v = Math.round(b) - Math.round(a);
  if (!v) return { texte: '0', classe: '' };
  return { texte: `${v < 0 ? '−' : '+'}${nb(Math.abs(v))}`, classe: v < 0 ? 'moins' : 'plus' };
}

export async function rendre(cible, etat, { appeler, afficher }) {
  const d = lire();
  const nomDeClasse = (id) => (((etat.classes && etat.classes.classes) || [])
    .find((c) => c.id === id) || {}).label || CLASSES[id] || '';
  // Le build chargé sans être enregistré compte comme un personnage de plus, en
  // tête : c'est lui qu'on confronte à un build enregistré.
  const charge = etat.build && etat.resolu && !courant(d) ? {
    id: CHARGE,
    nom: 'Build chargé',
    classe: etat.resolu.class,
    niveau: etat.resolu.level,
    builds: [{ id: CHARGE, nom: etat.nomDuBuild || 'non enregistré', charge: etat.build }],
  } : null;
  const personnages = [...(charge ? [charge] : []), ...d.personnages];
  const builds = personnages.flatMap((p) => p.builds.map((b) => ({ p, b })));
  const deLaClasse = (c) => builds.filter((x) => x.p.classe === c);
  const classes = [...new Set(builds.map((x) => x.p.classe))].filter((c) => deLaClasse(c).length >= 2);
  if (!classes.length) {
    cible.innerHTML = `
      <section class="outil comparaison">
        <h2>Comparaison</h2>
        <p class="sous">Enregistrez au moins deux builds d'une même classe pour les comparer.</p>
        <button type="button" class="action" id="versPersonnages">Gérer les personnages</button>
      </section>`;
    cible.querySelector('#versPersonnages').addEventListener('click', () => afficher('personnages'));
    return;
  }

  // Le choix tient pour la session : A, le build chargé s'il compte, sinon le
  // premier ; B, un autre build de la même classe. Un build chargé depuis rend
  // le résultat caduc et, non enregistré, devient A.
  const choix = etat.comparaison || (etat.comparaison = {});
  if (choix.resultat && choix.resultat.charge !== etat.build) choix.resultat = null;
  if (choix.vu !== etat.build) {
    choix.vu = etat.build;
    if (charge) choix.a = null;
  }
  const trouver = (c) => c && builds.find((x) => x.p.id === c.perso && x.b.id === c.build);
  const premierAutre = (a) => deLaClasse(a.p.classe).find((x) => x.b.id !== a.b.id);
  let a = trouver(choix.a);
  if (!a || !classes.includes(a.p.classe)) {
    const c = courant(d);
    a = (charge && classes.includes(charge.classe) && trouver({ perso: CHARGE, build: CHARGE }))
      || (c && classes.includes(c.perso.classe) && trouver({ perso: c.perso.id, build: c.build.id }))
      || deLaClasse(classes[0])[0];
  }
  let b = trouver(choix.b);
  if (!b || b.p.classe !== a.p.classe || b.b.id === a.b.id) b = premierAutre(a);
  choix.a = { perso: a.p.id, build: a.b.id };
  choix.b = { perso: b.p.id, build: b.b.id };

  const reglages = reglagesRotation(etat);
  const horizon = reglages.horizon || 7;
  const cibles = reglages.cibles || 1;
  const position = reglages.position || 'libre';
  const contre = decrire(versMoteur(reglages.resistances, true));
  const memesSorts = reglages.classe === a.p.classe && reglages.coches;
  const nom = (x) => `${x.p.nom} · ${x.b.nom}`;
  const persos = (classe) => personnages.filter((p) => p.builds.length && (classe
    ? p.classe === classe : classes.includes(p.classe)));
  const selecteur = (lettre, x, liste, exclu) => `
    <section class="encart choix-build"><h4>Build ${lettre}</h4><div>
      <label for="perso${lettre}">Personnage</label>
      <select id="perso${lettre}">${liste.map((p) => `<option value="${echapper(p.id)}"${p.id === x.p.id ? ' selected' : ''}>${
        echapper(p.nom)} · ${echapper(nomDeClasse(p.classe))} ${p.niveau}</option>`).join('')}</select>
      <label for="build${lettre}">Build</label>
      <select id="build${lettre}">${x.p.builds.filter((bb) => bb.id !== exclu).map((bb) => `<option value="${
        echapper(bb.id)}"${bb.id === x.b.id ? ' selected' : ''}>${echapper(bb.nom)}</option>`).join('')}</select>
    </div></section>`;

  cible.innerHTML = `
    <section class="outil comparaison">
      <h2>Comparaison</h2>
      <p class="sous">Avec les réglages de la Rotation : ${horizon} ${pluriel(horizon, 'tour')}, ${cibles} ${
        pluriel(cibles, 'ennemi')}, position ${POSITIONS[position] || POSITIONS.libre}, ${
        contre ? `contre ${echapper(contre)}` : 'cible sans résistance'}, ${
        memesSorts ? 'les sorts cochés dans la Rotation' : 'les sorts de l\'orientation de chaque build'}.</p>
      <div class="choix-builds">
        ${selecteur('A', a, persos(null), null)}
        ${selecteur('B', b, persos(a.p.classe), a.p.id === b.p.id ? a.b.id : null)}
        <div class="lancer">
          <button type="button" class="action" id="comparer">Comparer</button>
          <button type="button" class="action discret" id="annulerComparaison" hidden>Annuler</button>
        </div>
      </div>
      <p class="aide" id="etatComparaison"></p>
      <div id="resultatComparaison"></div>
    </section>`;

  const $ = (id) => cible.querySelector(`#${id}`);
  const redessiner = () => rendre(cible, etat, { appeler, afficher });
  $('persoA').addEventListener('change', () => {
    const p = personnages.find((x) => x.id === $('persoA').value);
    choix.a = { perso: p.id, build: p.builds[0].id };
    choix.resultat = null;
    redessiner();
  });
  $('buildA').addEventListener('change', () => {
    choix.a = { perso: a.p.id, build: $('buildA').value };
    choix.resultat = null;
    redessiner();
  });
  $('persoB').addEventListener('change', () => {
    const p = personnages.find((x) => x.id === $('persoB').value);
    choix.b = { perso: p.id, build: (p.builds.find((x) => x.id !== a.b.id) || p.builds[0]).id };
    choix.resultat = null;
    redessiner();
  });
  $('buildB').addEventListener('change', () => {
    choix.b = { perso: b.p.id, build: $('buildB').value };
    choix.resultat = null;
    redessiner();
  });
  $('annulerComparaison').addEventListener('click', async () => {
    $('annulerComparaison').disabled = true;
    try { await appeler('annuler'); } catch (e) { /* le calcul finira de lui-même */ }
  });

  // Un build calculé comme la Rotation le ferait : ses sorts cochés s'ils sont
  // de cette classe, sinon son orientation ; le mur de bombes réglé, sinon deux
  // Explobombes.
  async function calculer(x) {
    const resolu = await appeler('resoudre', x.b.charge);
    if (resolu.error) throw new Error(resolu.error);
    const classe = ((etat.classes && etat.classes.classes) || []).find((c) => c.id === resolu.class);
    const sorts = sortsDuBuild(classe, resolu);
    const coches = reglages.classe === resolu.class && reglages.coches ? reglages.coches : cochesParDefaut(sorts, resolu);
    const declares = (classe && classe.etats_declares) || [];
    const reponse = await appeler('rotation', {
      ...x.b.charge,
      deck: [...coches].filter((id) => sorts.some((s) => s.id === id)).slice(0, MAX_SORTS),
      horizon,
      pm_depenses: reglages.pmDepenses || 0,
      maitrise_d_arme: reglages.maitrise ?? 300,
      position,
      poussees_bloquees: Boolean(reglages.pousseesBloquees),
      targets: cibles,
      etalement: reglages.etalement || 0,
      exhaustif: Boolean(reglages.exhaustif),
      etats_declares: Object.fromEntries(declares.map((e) => [e.id,
        (reglages.etats && reglages.etats[e.id] != null) ? reglages.etats[e.id] : (Number(e.defaut) || 0)])),
      ...(resolu.class === ROUBLARD ? { mur_de_bombes: reglages.mur || { element: 'feu', combos: [1, 1] } } : {}),
      ...versMoteur(reglages.resistances, Boolean(resolu.arme)),
    });
    if (reponse.error) throw new Error(reponse.error);
    return reponse.rotation;
  }

  function dessiner(ra, rb) {
    const lignes = [
      [`Dégâts sur ${ra.turns.length} ${pluriel(ra.turns.length, 'tour')}`, ra.total, rb.total, true],
      ['Par tour', ra.per_turn, rb.per_turn, true],
      ...(ra.steady && ra.steady.per_turn && rb.steady && rb.steady.per_turn
        ? [['Sur la durée', ra.steady.per_turn, rb.steady.per_turn, true]] : []),
      ['PA perdus', ra.ap_wasted || 0, rb.ap_wasted || 0, false],
    ];
    // Les sources de la Répartition des deux rotations, alignées par
    // identifiant : sorts, poisons et invocations.
    const la = lancers(ra);
    const lb = lancers(rb);
    const sources = new Map();
    [[ra, 'a'], [rb, 'b']].forEach(([r, cote]) => (r.repartition || []).forEach((p) => {
      const s = sources.get(p.id) || { nom: p.name, a: null, b: null };
      s[cote] = p.damage;
      sources.set(p.id, s);
    }));
    const lignesParSort = [...sources.entries()]
      .sort(([, x], [, y]) => Math.max(y.a || 0, y.b || 0) - Math.max(x.a || 0, x.b || 0));
    const plusFort = Math.max(1, ...lignesParSort.map(([, s]) => Math.max(s.a || 0, s.b || 0)));
    const barre = (v, cote) => (v == null
      ? '<div class="ligne"><div class="piste"></div><span class="absent">·</span></div>'
      : `<div class="ligne"><div class="piste"><div class="barre ${cote}" style="width:${
        ((v / plusFort) * 100).toFixed(1)}%"></div></div><span>${nb(v)}</span></div>`);
    $('resultatComparaison').innerHTML = `
      <section class="encart resultat-comparaison"><h4>Résultat</h4><div>
        <table class="comparaison"><thead><tr><th></th><th>A</th><th>B</th><th>Écart</th></tr></thead><tbody>
        ${lignes.map(([titre, va, vb, enPourcent]) => {
          const e = enPourcent ? ecart(va, vb) : { texte: '', classe: '' };
          return `<tr><td>${titre}</td><td>${nb(va)}</td><td>${nb(vb)}</td><td class="${e.classe}">${e.texte}</td></tr>`;
        }).join('')}
        </tbody></table>
        <div class="copie-discord">
          <button type="button" class="action discret" id="copierComparaison">Copier pour Discord</button>
          <span class="copie" id="copieComparaison" role="status"></span>
        </div></div></section>
      <section class="encart"><h4>Dégâts par sort</h4><div>
        <div class="g-legende"><span><i class="g-puce g-p-a"></i>A · ${echapper(nom(a))}</span>
          <span><i class="g-puce g-p-b"></i>B · ${echapper(nom(b))}</span></div>
        <table class="comparaison par-sort"><thead><tr><th>Source</th><th>Lancers A / B</th><th class="degats">Dégâts</th>
          <th>Écart</th></tr></thead><tbody>
        ${lignesParSort.map(([id, s]) => {
          const e = ecartEnPoints(s.a || 0, s.b || 0);
          return `<tr><td>${echapper(s.nom)}</td><td>${la.get(id) || 0} / ${lb.get(id) || 0}</td>
            <td class="barres">${barre(s.a, 'a')}${barre(s.b, 'b')}</td><td class="${e.classe}">${e.texte}</td></tr>`;
        }).join('')}
        </tbody></table></div></section>
      <section class="encart"><h4>Dégâts par tour</h4><div>
        ${deuxBuilds(ra.turns.map((t) => t.damage), rb.turns.map((t) => t.damage), nom(a), nom(b))}
      </div></section>`;
    $('copierComparaison').addEventListener('click', async () => {
      const texte = discord.comparaison({
        ra,
        rb,
        demande: { horizon, targets: cibles, position, ...versMoteur(reglages.resistances, true) },
        titre: `Comparaison ${nomDeClasse(a.p.classe)} ${a.p.niveau}`,
        nomA: nom(a),
        nomB: nom(b),
        version: await discord.version(appeler),
      });
      const ok = await discord.copier(texte);
      $('copieComparaison').className = `copie ${ok ? 'ok' : 'refus'}`;
      $('copieComparaison').textContent = ok ? 'Copié.' : 'Le système refuse la copie.';
    });
  }

  if (choix.resultat) dessiner(choix.resultat.a, choix.resultat.b);

  $('comparer').addEventListener('click', async () => {
    $('comparer').disabled = true;
    $('annulerComparaison').hidden = false;
    $('annulerComparaison').disabled = false;
    try {
      $('etatComparaison').textContent = `Calcul de A, ${nom(a)}…`;
      const ra = await calculer(a);
      $('etatComparaison').textContent = `Calcul de B, ${nom(b)}…`;
      const rb = await calculer(b);
      choix.resultat = { a: ra, b: rb, charge: etat.build };
      $('etatComparaison').textContent = '';
      dessiner(ra, rb);
    } catch (e) {
      $('etatComparaison').textContent = e.message === 'calcul annulé' ? 'Calcul annulé.' : e.message;
    } finally {
      $('comparer').disabled = false;
      $('annulerComparaison').hidden = true;
    }
  });
}
