// Ce que la Rotation laisse au compagnon de combat : la rotation affichée, tour
// par tour, dans le stockage local que ses deux fenêtres partagent. Le
// compagnon la relit à chaque nouveau calcul.

import { hote } from '../pont.js';
import { lireLesTouches } from './touches.js';

export const CLE = 'krozties.compagnon';

/// La rotation, réduite à ce que le compagnon montre.
export function publier(r, build) {
  const suivi = {
    calcule_le: Date.now(),
    build,
    tours: (r.turns || []).map((t) => ({
      degats: Math.round(t.damage),
      pa: t.casts.reduce((s, c) => s + (c.ap || 0), 0),
      lancers: t.casts.map((c) => ({
        id: c.id,
        nom: c.name,
        pa: c.ap,
        icone: c.dofusdb_id || null,
        objet: c.icone_objet || null,
        indications: c.indications || [],
      })),
    })),
  };
  try {
    localStorage.setItem(CLE, JSON.stringify(suivi));
  } catch (e) {
    // Stockage refusé : le compagnon reste vide, la Rotation n'en souffre pas.
  }
}

/// Le compagnon, par-dessus le jeu dans l'application de bureau, avec les
/// touches choisies ; dans une fenêtre du navigateur sinon.
export async function ouvrirLeCompagnon(appeler) {
  if (hote === 'bureau') return appeler('compagnon', { touches: lireLesTouches() });
  window.open('compagnon.html', 'compagnon', 'width=460,height=140');
  return null;
}
