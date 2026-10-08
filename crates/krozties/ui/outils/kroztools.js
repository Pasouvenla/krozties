// KrozTools : les simulateurs sur damier, un onglet par outil (KrozZone, KrozTrap,
// KrozBoom, KrozPortal, KrozSight). Un onglet sans module reste grisé et marqué
// « à venir ». Chaque outil est son propre module ; ce fichier ne tient que le
// bandeau et l'onglet ouvert.

const ONGLETS = [
  { id: 'zone', nom: 'KrozZone', quoi: 'Zones de sorts', module: () => import('./zones.js') },
  { id: 'trap', nom: 'KrozTrap', quoi: 'Pièges du Sram', module: () => import('./reseau.js') },
  { id: 'boom', nom: 'KrozBoom', quoi: 'Bombes du Roublard', module: () => import('./bombes.js') },
  { id: 'portal', nom: 'KrozPortal', quoi: "Portails de l'Éliotrope", module: () => import('./portails.js') },
  { id: 'vue', nom: 'KrozSight', quoi: 'Ligne de vue', module: () => import('./vue.js') },
];

export async function rendre(cible, etat, contexte) {
  const pret = (o) => Boolean(o && o.module);
  let courant = ONGLETS.find((o) => o.id === etat.kroztool && pret(o)) || ONGLETS[0];

  cible.innerHTML = `
    <section class="outil kroztools">
      <h2>KrozTools</h2>
      <div class="onglets" role="tablist" aria-label="Outils KrozTools">
        ${ONGLETS.map((o) => `<button type="button" role="tab" id="onglet-${o.id}"
          data-onglet="${o.id}" aria-controls="panneauKroz"
          ${pret(o) ? '' : 'disabled aria-disabled="true"'}>
          <span class="nom">${o.nom}</span>
          <span class="quoi">${o.quoi}${pret(o) ? '' : ' · à venir'}</span>
        </button>`).join('')}
      </div>
      <div id="panneauKroz" role="tabpanel"></div>
    </section>`;

  const bandeau = cible.querySelector('.onglets');
  const panneau = cible.querySelector('#panneauKroz');

  async function ouvrir(onglet, focaliser) {
    courant = onglet;
    etat.kroztool = onglet.id;
    bandeau.querySelectorAll('[role=tab]').forEach((b) => {
      const choisi = b.dataset.onglet === onglet.id;
      b.setAttribute('aria-selected', String(choisi));
      b.tabIndex = choisi ? 0 : -1;
      if (choisi && focaliser) b.focus();
    });
    panneau.setAttribute('aria-labelledby', `onglet-${onglet.id}`);
    panneau.innerHTML = '<p class="aide">Chargement…</p>';
    const mod = await onglet.module();
    // Un clic sur un autre onglet pendant le chargement l'emporte.
    if (courant !== onglet) return;
    await mod.rendre(panneau, etat, contexte);
  }

  bandeau.addEventListener('click', (e) => {
    const b = e.target.closest('[role=tab]');
    if (!b || b.disabled) return;
    const o = ONGLETS.find((x) => x.id === b.dataset.onglet);
    if (o !== courant) ouvrir(o, false);
  });
  // Le bandeau se parcourt aux flèches, comme tout groupe d'onglets.
  bandeau.addEventListener('keydown', (e) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    const prets = ONGLETS.filter(pret);
    const i = prets.indexOf(courant);
    const suivant = e.key === 'Home' ? prets[0]
      : e.key === 'End' ? prets[prets.length - 1]
        : prets[(i + (e.key === 'ArrowRight' ? 1 : prets.length - 1)) % prets.length];
    e.preventDefault();
    if (suivant !== courant) ouvrir(suivant, true);
  });

  await ouvrir(courant, false);
}
