// La page de test de l'interface : chaque outil et chaque onglet de KrozTools
// s'affichent, et le générateur de KrozTrap va jusqu'au bout.
//
// Chargée par `crates/cli/tests/interface.rs` dans Chrome sans fenêtre, à côté
// de `app.js`. Elle n'écrit que dans la page : le test relit le résultat dans
// le document que Chrome rend.

const erreurs = [];
addEventListener('error', (e) => erreurs.push(`${e.message} (${e.filename}:${e.lineno})`));
addEventListener('unhandledrejection', (e) => {
  erreurs.push(`promesse rejetée : ${(e.reason && e.reason.message) || e.reason}`);
});

const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const texte = (sel) => {
  const e = document.querySelector(sel);
  return e ? e.innerText.replace(/\s+/g, ' ').trim() : null;
};
async function attendre(condition, max) {
  for (let t = 0; t < max; t += 100) {
    if (condition()) return true;
    await pause(100);
  }
  return false;
}

const resultat = { outils: {}, onglets: {}, reseau: {}, portail: {}, carte: {}, erreurs };
try {
  const app = await import('/app.js');
  const { appeler } = await import('/pont.js');

  // La mini fenêtre de mise à jour : la comparaison des numéros, puis son texte.
  const maj = await import('/commun/mise-a-jour.js');
  resultat.majComparee = [['0.10.0', '0.9.9'], ['0.1.0', '0.1.0'], ['1.0', '0.12.3'], ['0.1.0', '0.2.0']]
    .map(([a, b]) => maj.plusRecente(a, b));
  const fenetre = maj.afficher('0.2.0', '0.1.0');
  resultat.majTexte = fenetre.innerText.replace(/\s+/g, ' ').trim();
  fenetre.remove();

  // La vérification part à chaque ouverture, et sans réseau le numéro déjà
  // lu reste. GitHub est simulé ; la version installée, que seule
  // l'application de bureau connaît, arrête ensuite chaque passage, ce qui ne
  // change rien à ce qu'il a retenu.
  const CLE_MAJ = 'krozties.mise-a-jour';
  const vraiFetch = window.fetch;
  const demandes = [];
  let horsLigne = false;
  localStorage.removeItem(CLE_MAJ);
  window.fetch = async (url, options) => {
    if (!String(url).includes('api.github.com')) return vraiFetch(url, options);
    if (horsLigne) throw new TypeError('hors ligne');
    demandes.push(String(url));
    return new Response(JSON.stringify({ tag_name: 'v9.9.9' }), { status: 200 });
  };
  try {
    await maj.verifier().catch(() => {});
    await maj.verifier().catch(() => {});
    horsLigne = true;
    await maj.verifier().catch(() => {});
    resultat.majVerifiee = {
      demandes: demandes.length,
      derniere: JSON.parse(localStorage.getItem(CLE_MAJ) || '{}').derniere ?? null,
    };
  } finally {
    window.fetch = vraiFetch;
    localStorage.removeItem(CLE_MAJ);
  }

  // Un Sram de niveau 200, comme les tests du moteur : la Rotation exige un
  // équipement pour s'ouvrir.
  const build = {
    class: 4,
    level: 200,
    invested: { agility: 300, strength: 300, intelligence: 200, chance: 200 },
  };
  app.etat.build = build;
  app.etat.resolu = await appeler('resoudre', build);
  app.majIdentite();

  for (const outil of ['equipement', 'rotation', 'beta', 'kroztools']) {
    await app.afficher(outil);
    await pause(300);
    resultat.outils[outil] = {
      titre: texte('#contenu h2'),
      sous: texte('#contenu .sous'),
      elements: document.querySelectorAll('#contenu *').length,
    };
  }

  // Chaque onglet de KrozTools qui s'ouvre, par un clic comme le joueur.
  for (const onglet of ['zone', 'trap', 'boom', 'portal', 'vue']) {
    document.getElementById(`onglet-${onglet}`).click();
    await pause(600);
    resultat.onglets[onglet] = {
      choisi: document.getElementById(`onglet-${onglet}`).getAttribute('aria-selected'),
      elements: document.querySelectorAll('#panneauKroz *').length,
    };
  }
  // L'aperçu de pose suit le curseur dans chaque outil à damier : survolée,
  // une case montre ce qu'un clic y poserait.
  resultat.apercus = {};
  for (const onglet of ['zone', 'trap', 'boom', 'portal', 'vue']) {
    document.getElementById(`onglet-${onglet}`).click();
    await pause(600);
    const cases = document.querySelectorAll('#panneauKroz polygon.case');
    cases[Math.floor(cases.length / 3)].dispatchEvent(new MouseEvent('mouseenter'));
    const couche = document.querySelector('#panneauKroz g.apercu-pose');
    resultat.apercus[onglet] = Boolean(couche && couche.innerHTML);
  }

  // KrozPortal s'ouvre sur son exemple : deux portails à huit cases d'écart.
  // La classe du lanceur s'y choisit, pour les sorts d'un allié.
  document.getElementById('onglet-portal').click();
  await pause(600);
  resultat.portail.bonus = texte('#bonusPortail dd');
  resultat.portail.classes = document.querySelectorAll('#classePortail option').length;

  // KrozBoom s'ouvre sur son exemple : deux Explobombes, combos III et II,
  // un mur entre elles et un ennemi dedans.
  document.getElementById('onglet-boom').click();
  await pause(1500);
  const ligneBoom = document.querySelector('#detailBoom tbody tr');
  resultat.boom = {
    explosion: ligneBoom && ligneBoom.children[1].textContent,
    mur: ligneBoom && ligneBoom.children[2].textContent,
    murs: document.querySelectorAll('#plateauBoom .mur-bombe').length,
  };
  // Deux Plombages sur une bombe : le mur repart deux fois, ses bombes
  // montées d'un cran (IV et III).
  document.querySelector('input[name=plombageBoom][value="2"]').click();
  await pause(800);
  const ligneBoomPlombage = document.querySelector('#detailBoom tbody tr');
  resultat.boom.plombage = ligneBoomPlombage && ligneBoomPlombage.children[4]
    && ligneBoomPlombage.children[4].textContent;

  // KrozZone chiffre les poisons : le Vent Empoisonné du Sadida frappe deux
  // fois, et la carte le dit sur l'ennemi posé d'office sur la visée.
  document.getElementById('onglet-zone').click();
  await pause(600);
  const classeZone = document.getElementById('classeZone');
  classeZone.value = '10';
  classeZone.dispatchEvent(new Event('change'));
  await pause(600);
  const sortZone = document.getElementById('sortChoisi');
  sortZone.value = 'vent_empoisonne';
  sortZone.dispatchEvent(new Event('change'));
  await pause(1500);
  resultat.poison = {
    etiquettes: [...document.querySelectorAll('#plateauZone text')]
      .map((t) => t.textContent).filter((t) => t.includes('×')),
    quand: texte('#detailZone p.aide'),
  };

  // Chaque fiche de sort se compose, pour les dix-neuf classes, avec et
  // sans build : une constante disparue y laisserait une erreur sur toute fiche
  // à effet critique. On y vérifie aussi qu'aucune ligne ne commence par une
  // minuscule.
  const { ficheDeSort } = await import('/commun/infobulle.js');
  const fiches = document.createElement('div');
  document.body.appendChild(fiches);
  resultat.fiches = { composees: 0, erreurs: [], minuscules: [] };
  for (const c of app.etat.classes.classes) {
    for (const s of c.spells || []) {
      // Sans build, puis sous et au-dessus du seuil du critique sûr : les
      // trois branches de la phrase des effets critiques.
      for (const opts of [{}, ...[40, 90].map((critique) => (
        { critique, tables: app.etat.resolu.damage_tables, certain: 78 }))]) {
        try {
          fiches.innerHTML = ficheDeSort(s, opts);
          resultat.fiches.composees += 1;
        } catch (e) {
          resultat.fiches.erreurs.push(`${c.label} / ${s.name} : ${e.message}`);
          continue;
        }
        fiches.querySelectorAll('h4, div, p, dt, dd').forEach((el) => {
          const premier = [...el.childNodes].find((n) => (n.textContent || '').trim());
          const t = premier ? premier.textContent.trim() : '';
          if (/^[a-zàâäçéèêëîïôöùûüœ]/.test(t)) resultat.fiches.minuscules.push(`${s.name} : ${t.slice(0, 40)}`);
        });
      }
    }
  }
  fiches.remove();

  // Les objets de classe d'un Crâ : la fiche de l'objet dit ce qu'il change
  // aux sorts, et l'infobulle du sort le montre.
  const cra = await appeler('resoudre', {
    class: 9, level: 200, items: [8636, 0, 0, 0, 8660], invested: { intelligence: 300 },
  });
  const ficheCra = document.createElement('div');
  document.body.appendChild(ficheCra);
  const rangee = (titre) => {
    const dt = [...ficheCra.querySelectorAll('dt')].find((d) => d.textContent === titre);
    return dt && dt.nextElementSibling ? dt.nextElementSibling.innerText.replace(/\s+/g, ' ').trim() : null;
  };
  const sortCra = (id) => app.etat.classes.classes.find((c) => c.id === 9).spells.find((s) => s.id === id);
  const avecCra = { critique: 30, tables: cra.damage_tables, certain: 78, modifs: cra.sorts_modifies };
  ficheCra.innerHTML = ficheDeSort(sortCra('fleche_ralentissante'), avecCra);
  resultat.objetsDeClasse = { equipement: rangee('Équipement') };
  ficheCra.innerHTML = ficheDeSort(sortCra('pluie_de_fleches'), avecCra);
  resultat.objetsDeClasse.critique = rangee('Coup critique');
  resultat.objetsDeClasse.fiche = (cra.stuff.find((e) => e.slot === 'ch') || {}).sorts;
  ficheCra.remove();

  // Une carte de boss choisie dans KrozSight : son sol et ses murs se
  // dessinent, et ce que la case ne voit pas s'ombre.
  document.getElementById('onglet-vue').click();
  await pause(600);
  const choix = document.getElementById('choixPlateauVue');
  choix.value = 'carte:62';
  choix.dispatchEvent(new Event('change'));
  await attendre(() => document.querySelectorAll('#plateauVue g.bloc').length > 0, 5000);
  await pause(300);
  resultat.carte = {
    sol: document.querySelectorAll('#plateauVue polygon.case').length,
    murs: document.querySelectorAll('#plateauVue g.bloc').length,
    ombre: document.querySelectorAll('#plateauVue .case.cachee').length,
    choisie: choix.options[choix.selectedIndex].textContent,
  };
  resultat.onglets.fermes = [...document.querySelectorAll('.onglets [role=tab][disabled]')]
    .map((b) => b.dataset.onglet);

  // Le générateur, à la main : une entrée, quatre PA, une proposition, un
  // tour posé.
  app.etat.reseau = {
    mode: 'main', lanceur: [0, 0], poses: [], entree: [3, 0], obstacles: [], choisi: null,
    chakra: false, allies: [], ennemis: [], pa: 4,
  };
  app.etat.kroztool = 'trap';
  await app.afficher('kroztools');
  await pause(300);
  document.getElementById('proposerTour').click();
  await attendre(() => !document.getElementById('propositionReseau').hidden, 60000);
  resultat.reseau.proposition = texte('#propositionReseau');
  const poser = document.querySelector('#propositionReseau button[data-action=poser]');
  if (poser) poser.click();
  await attendre(() => texte('#tourCourant') === 'Tour 2', 20000);
  resultat.reseau.apres = { tour: texte('#tourCourant'), etat: texte('#etatReseau') };

  // « Limiter à l'orientation » sur un Sram Eau : ses sorts et ses pièges
  // Eau, pas les autres.
  const eau = { class: 4, level: 200, invested: { chance: 390 } };
  app.etat.build = eau;
  app.etat.resolu = await appeler('resoudre', eau);
  await app.afficher('rotation');
  await pause(300);
  document.getElementById('orientation').click();
  await pause(300);
  resultat.orientation = [...app.etat.rotation.coches].sort();

  // Votre position : trois boutons, et « Distance » dit ce qui sort de la
  // rotation et passe dans le réglage.
  const positions = [...document.querySelectorAll('input[name="position"]')];
  const aDistance = positions.find((p) => p.value === 'distance');
  aDistance.click();
  resultat.position = {
    choix: positions.map((p) => p.parentElement.innerText.trim()),
    aide: texte('#aidePosition'),
    reglage: app.etat.rotation.position,
  };
  positions.find((p) => p.value === 'libre').click();

  // Le concepteur : un réseau entier sur le damier, en deux tours de quatre
  // PA, qu'on parcourt dans les deux sens jusqu'au déclenchement et ses
  // entrées. Le joueur ne donne pas d'entrée.
  app.etat.reseau.mode = 'concevoir';
  app.etat.kroztool = 'trap';
  await app.afficher('kroztools');
  await pause(300);
  document.getElementById('toursReseau').value = '2';
  document.getElementById('toursReseau').dispatchEvent(new Event('change'));
  document.getElementById('concevoirReseau').click();
  await attendre(() => !document.getElementById('concevoirReseau').disabled
    && document.querySelector('#planReseau button[data-pas]'), 120000);
  const pas = () => texte('#planReseau button[aria-pressed=true]');
  const avancer = (sens) => document.querySelector(`#planReseau button[data-aller="${sens}"]`).click();
  resultat.reseau.plan = {
    etapes: [...document.querySelectorAll('#planReseau button[data-pas]')].map((b) => b.textContent),
    pas: pas(),
    resume: texte('#planReseau .bandeau'),
    panneau: texte('#panneauPlan'),
    numeros: document.querySelectorAll('#plateauReseau .piege-groupe.du-tour').length,
  };
  avancer(1);
  await attendre(() => pas() === 'Tour 2', 20000);
  resultat.reseau.plan.suivant = texte('#panneauPlan');
  avancer(1);
  await attendre(() => pas() === 'Déclenchement', 20000);
  await attendre(() => document.querySelector('#panneauPlan .pile.entrees'), 20000);
  resultat.reseau.plan.fin = texte('#panneauPlan');
  resultat.reseau.plan.entrees = document.querySelectorAll('#plateauReseau .entree-contour').length;
  avancer(-1);
  await attendre(() => pas() === 'Tour 2', 20000);
  resultat.reseau.plan.retour = pas();

  // Les bonus de Dofus qui dépendent du combat ont leur case dans la Rotation,
  // pour les Dofus du build chargé : le Pandawa en porte quatre
  // (Vulbis, Ocre, Turquoise, Pourpre). Décocher une case l'écarte du calcul.
  // En dernier : ce build remplace le Sram des étapes précédentes.
  const pandawa = {"v": 1, "src": "dofusbook", "id": 2, "short": "XXXXX", "class": 12, "level": 200, "items": [18590, 17103, 32234, 24035, 17104, 17105, 18591, 32236, 32235, 13673, 0, 7043, 6980, 739, 7754, 694, 8698], "carac": {"scroll_vi": 100, "scroll_sa": 100, "scroll_fo": 100, "scroll_in": 100, "scroll_ch": 100, "scroll_ag": 100, "base_vi": 0, "base_sa": 0, "base_fo": 0, "base_in": 0, "base_ch": 266, "base_ag": 265}, "fm": {"a1": {"dc": 8}, "a2": {"pa": 1}, "am": {"cc": 2}, "ar": {"dc": 8}, "bo": {"dc": 8}, "br": {"dc": 8}, "ca": {"pm": 1}, "ce": {"cc": 2}, "ch": {"dd": 1}, "d1": {}, "d2": {}, "d3": {"deg": 10}, "d4": {}, "d5": {"deg": 5}, "d6": {"deg": 20}, "fa": {}, "mo": {}}, "fmGlobal": {}, "boosts": [{"name": "Harmonie de Pandala", "stat": "dmg", "percent": 20, "class_id": null, "effect_id": "18888-1-1-1600-0"}, {"name": "Rêve Nébuleux", "stat": "deg", "percent": 20, "class_id": null, "effect_id": "5454-1-1-1630-0"}, {"name": "Bleu Turquoise", "stat": "deg", "percent": 10, "class_id": null, "effect_id": "5952-1-1-1630-0"}]};
  app.etat.build = pandawa;
  app.etat.resolu = await appeler('resoudre', pandawa);
  await app.afficher('rotation');
  await pause(400);
  const casesDofus = [...document.querySelectorAll('#contenu input[data-bonus]')];
  resultat.bonusDofus = {
    noms: casesDofus.map((c) => c.dataset.bonus),
    cochees: casesDofus.filter((c) => c.checked).length,
  };
  if (casesDofus[0]) {
    casesDofus[0].click();
    resultat.bonusDofus.ecartes = app.etat.bonusEcartes || [];
  }

  // Le Roublard règle le mur que Plombage redéclenche, repris de KrozBoom :
  // son exemple a posé deux Explobombes aux combos III et II.
  const roublard = { class: 13, level: 200 };
  app.etat.build = roublard;
  app.etat.resolu = await appeler('resoudre', roublard);
  await app.afficher('rotation');
  await pause(400);
  const murElement = document.getElementById('murElement');
  resultat.murDeBombes = {
    element: murElement && murElement.value,
    combos: [...document.querySelectorAll('#contenu select[data-mur-combo]')].map((s) => s.value),
  };

  // L'Osamodas voit les dégâts de ses invocations : le Tofu, rang 3, à 50 %
  // de ses caractéristiques ; nu, Béco-béco garde sa base.
  const osamodas = { class: 2, level: 200 };
  app.etat.build = osamodas;
  app.etat.resolu = await appeler('resoudre', osamodas);
  await app.afficher('rotation');
  await attendre(() => !document.getElementById('carteInvocations').hidden, 5000);
  const ligneBeco = [...document.querySelectorAll('#carteInvocations tbody tr')]
    .find((tr) => tr.children[0].textContent.startsWith('Béco-béco'));
  resultat.invocations = {
    titre: `${texte('#carteInvocations .encart > h4')} | ${texte('#carteInvocations .encart .sort-source')}`,
    beco: ligneBeco && [...ligneBeco.children].slice(1).map((td) => td.textContent),
  };
  // Une commune ne s'affiche que cochée : l'Arakne, décochée puis cochée.
  const caseArakne = document.querySelector('#listeSorts input[value="invocation_de_l_arakne"]');
  const arakne = () => [...document.querySelectorAll('#carteInvocations .encart > h4')]
    .filter((h) => h.firstChild.textContent.trim() === 'Arakne').length;
  if (caseArakne.checked) caseArakne.click();
  const sansArakne = arakne();
  caseArakne.click();
  resultat.invocations.arakne = [sansArakne, arakne()];

  // Les conseils par tour : une rotation de deux tours sur quatre sorts, un
  // par paire de variantes (le jeu n'en laisse équiper qu'une), puis un clic
  // sur son premier lancer, qui ouvre ce que les autres sorts vaudraient à sa
  // place.
  const horizon = document.getElementById('horizon');
  horizon.value = '2';
  horizon.dispatchEvent(new Event('input', { bubbles: true }));
  [...document.querySelectorAll('#contenu label.sort input[type=checkbox]')].forEach((c, i) => {
    if (c.checked !== (i % 2 === 0 && i < 8)) c.click();
  });
  document.getElementById('calculer').click();
  await attendre(() => document.querySelector('#detailRotation .lancer[data-tour]'), 60000);
  document.querySelector('#detailRotation .lancer[data-tour]').click();
  await attendre(() => document.querySelector('#detailRotation .conseil table'), 20000);
  resultat.conseil = {
    lignes: document.querySelectorAll('#detailRotation .conseil tbody tr').length,
    joue: document.querySelectorAll('#detailRotation .conseil tr.joue').length,
  };

  // L'arme du build, un sort de plus au deck de la Rotation, avec son réglage
  // de Maîtrise et sa fiche : la Hachebarde de Guerre d'un Ouginak, lancée dans
  // un calcul de deux tours.
  const objets = Array(17).fill(0);
  objets[8] = 22368;
  // La Couronne de Brâm Barbe-Monde répond aux retraits de PM de l'arme.
  objets[0] = 20359;
  const ouginak = { class: 18, level: 200, items: objets, invested: { intelligence: 400 } };
  app.etat.build = ouginak;
  app.etat.resolu = await appeler('resoudre', ouginak);
  await app.afficher('rotation');
  await pause(400);
  const caseArme = document.querySelector('#contenu label.sort[data-sort="arme"] input');
  const ficheArme = document.createElement('div');
  ficheArme.innerHTML = ficheDeSort(app.etat.resolu.arme, {
    critique: app.etat.resolu.crit, tables: app.etat.resolu.damage_tables, certain: 78,
  });
  resultat.arme = {
    au_deck: Boolean(caseArme),
    cochee: Boolean(caseArme && caseArme.checked),
    maitrise: (document.getElementById('maitrise') || {}).value || null,
    titre: (ficheArme.querySelector('h4') || {}).textContent || null,
  };
  document.getElementById('horizon').value = '2';
  document.getElementById('horizon').dispatchEvent(new Event('input', { bubbles: true }));
  document.getElementById('calculer').click();
  await attendre(() => document.querySelector('#detailRotation .lancer[data-tour]'), 60000);
  resultat.arme.lancee = document.querySelectorAll('#detailRotation .lancer[data-sort="arme"]').length;
  // Les remarques du calcul, dans leur carte sous la répartition.
  resultat.remarques = [...document.querySelectorAll('#detailRotation .remarques li')].map((li) => li.textContent);

  // Le sort qu'un objet ajoute à la barre, au deck de la Rotation : la Pelle
  // Fantomatique de l'Épée Nécronyx, chez un Crâ Feu.
  const objetsCra = Array(17).fill(0);
  objetsCra[8] = 19482;
  const craFeu = { class: 9, level: 200, items: objetsCra, invested: { intelligence: 400 } };
  app.etat.build = craFeu;
  app.etat.resolu = await appeler('resoudre', craFeu);
  await app.afficher('rotation');
  await pause(400);
  const casePelle = document.querySelector('#contenu label.sort[data-sort="sort_d_objet_2056"] input');
  const fichePelle = document.createElement('div');
  fichePelle.innerHTML = ficheDeSort((app.etat.resolu.sorts_d_objets || [])[0] || {}, {
    critique: app.etat.resolu.crit, tables: app.etat.resolu.damage_tables, certain: 78,
  });
  resultat.sortDObjet = {
    au_deck: Boolean(casePelle),
    cochee: Boolean(casePelle && casePelle.checked),
    titre: (fichePelle.querySelector('h4') || {}).textContent || null,
  };

  // La fiche d'un objet au survol de son Dofus : le Dofus Ocre, son effet
  // spécial compté, et son attitude.
  objetsCra[11] = 7754;
  app.etat.resolu = await appeler('resoudre', craFeu);
  await app.afficher('equipement');
  await pause(400);
  const ocre = document.querySelector('[data-objet="d1"]');
  if (ocre) ocre.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
  await attendre(() => {
    const b = document.querySelector('.infobulle');
    return b && !b.hidden && b.querySelector('h4');
  }, 3000);
  const bulle = document.querySelector('.infobulle');
  resultat.ficheObjet = {
    titre: (bulle.querySelector('h4') || {}).textContent || null,
    statut: (bulle.querySelector('.statut') || {}).textContent || null,
    aussi: [...bulle.querySelectorAll('.section p')].map((p) => p.textContent).pop() || null,
  };
} catch (e) {
  erreurs.push(`exception : ${e.message}`);
}

const pre = document.createElement('pre');
pre.id = 'fumee';
pre.textContent = JSON.stringify(resultat);
document.body.appendChild(pre);
