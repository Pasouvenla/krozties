// Une nouvelle version publiée : une mini fenêtre, en bas à droite, l'annonce.
// L'application demande la dernière version à GitHub à chaque ouverture, et
// « Plus tard » la masque jusqu'au lendemain.

import { appeler } from '../pont.js';
import { echapper } from './format.js';

const DERNIERE = 'https://api.github.com/repos/Pasouvenla/krozties/releases/latest';
const JOUR = 24 * 60 * 60 * 1000;
const CLE = 'krozties.mise-a-jour';
const NUMERO = /^\d+(\.\d+){1,3}$/;

/// `a` est-elle plus récente que `b` ? Les numéros se comparent nombre par
/// nombre : 0.10.0 vient après 0.9.9.
export function plusRecente(a, b) {
  const pa = String(a).split('.').map((n) => parseInt(n, 10) || 0);
  const pb = String(b).split('.').map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    const ecart = (pa[i] || 0) - (pb[i] || 0);
    if (ecart) return ecart > 0;
  }
  return false;
}

function lire() {
  try {
    return JSON.parse(localStorage.getItem(CLE)) || {};
  } catch (e) {
    return {};
  }
}

function ecrire(memoire) {
  try {
    localStorage.setItem(CLE, JSON.stringify(memoire));
  } catch (e) {
    // Stockage refusé : la vérification recommencera au prochain lancement.
  }
}

/// La mini fenêtre : `derniere` est publiée, `actuelle` est installée.
export function afficher(derniere, actuelle) {
  document.querySelector('.maj')?.remove();
  const fenetre = document.createElement('aside');
  fenetre.className = 'maj';
  fenetre.setAttribute('role', 'status');
  fenetre.innerHTML = `
    <strong>Krozties ${echapper(derniere)} est disponible</strong>
    <span class="sous">Vous utilisez la version ${echapper(actuelle)}.</span>
    <div class="actions">
      <button type="button" class="telecharger">Télécharger</button>
      <button type="button" class="plus-tard">Plus tard</button>
    </div>`;
  const masquer = () => {
    ecrire({ ...lire(), masquee_le: Date.now() });
    fenetre.remove();
  };
  fenetre.querySelector('.telecharger').addEventListener('click', () => {
    appeler('versions').catch(() => {});
    masquer();
  });
  fenetre.querySelector('.plus-tard').addEventListener('click', masquer);
  document.body.appendChild(fenetre);
  return fenetre;
}

/// À chaque démarrage de l'application de bureau. Sans réseau, ou si GitHub
/// refuse, le numéro lu à une ouverture précédente sert encore.
export async function verifier() {
  const memoire = lire();
  try {
    const reponse = await fetch(DERNIERE, { headers: { Accept: 'application/vnd.github+json' } });
    if (reponse.ok) {
      const numero = String((await reponse.json()).tag_name || '').replace(/^v/, '');
      memoire.derniere = NUMERO.test(numero) ? numero : null;
      ecrire(memoire);
    }
  } catch (e) {
    // Pas de réseau : rien à mettre à jour.
  }
  if (!memoire.derniere) return;
  if (memoire.masquee_le && Date.now() - memoire.masquee_le < JOUR) return;
  const actuelle = await appeler('version');
  if (plusRecente(memoire.derniere, actuelle)) afficher(memoire.derniere, actuelle);
}
