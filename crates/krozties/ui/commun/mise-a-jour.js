// Une nouvelle version publiée : une mini fenêtre, en bas à droite, l'annonce.
// L'application demande la dernière version à GitHub à chaque ouverture, et
// « Plus tard » la masque jusqu'au lendemain. Quand la version publiée porte sa
// mise à jour signée, l'application la télécharge et l'installe elle-même ; le
// joueur choisit quand redémarrer. Sur Windows, l'installation attend ce
// redémarrage, ou la fermeture.

import { appeler } from '../pont.js';
import { echapper, majuscule } from './format.js';

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

const mo = (octets) => `${Math.round(octets / 1e6)} Mo`;

/// La mini fenêtre : `derniere` est publiée, `actuelle` est installée ; `auto`,
/// l'application sait l'installer elle-même.
export function afficher(derniere, actuelle, auto = false) {
  document.querySelector('.maj')?.remove();
  const fenetre = document.createElement('aside');
  fenetre.className = 'maj';
  fenetre.setAttribute('role', 'status');
  fenetre.innerHTML = `
    <strong>Krozties ${echapper(derniere)} est disponible</strong>
    <span class="sous">Vous utilisez la version ${echapper(actuelle)}.</span>
    <div class="actions">
      <button type="button" class="telecharger">${auto ? 'Mettre à jour' : 'Télécharger'}</button>
      <button type="button" class="plus-tard">Plus tard</button>
    </div>`;
  const masquer = () => {
    ecrire({ ...lire(), masquee_le: Date.now() });
    fenetre.remove();
  };
  fenetre.querySelector('.telecharger').addEventListener('click', () => {
    if (auto) {
      installer(fenetre, derniere);
      return;
    }
    appeler('versions').catch(() => {});
    masquer();
  });
  fenetre.querySelector('.plus-tard').addEventListener('click', masquer);
  document.body.appendChild(fenetre);
  return fenetre;
}

/// Le téléchargement et l'installation, suivis dans la fenêtre.
export async function installer(fenetre, derniere) {
  fenetre.innerHTML = `
    <strong>Mise à jour vers la ${echapper(derniere)}</strong>
    <span class="detail">Téléchargement…</span>
    <div class="jauge"><span></span></div>`;
  const suivi = setInterval(async () => {
    try {
      const a = await appeler('majAvancement');
      if (!a.total) return;
      fenetre.querySelector('.jauge span').style.width = `${Math.min(100, (a.recu / a.total) * 100).toFixed(1)}%`;
      fenetre.querySelector('.detail').textContent = `Téléchargement : ${mo(a.recu)} sur ${mo(a.total)}`;
    } catch (e) {
      // Le téléchargement prime : une lecture ratée ne l'interrompt pas.
    }
  }, 250);
  try {
    pret(fenetre, derniere, await appeler('majInstaller'));
  } catch (e) {
    echec(fenetre, derniere, e.message);
  } finally {
    clearInterval(suivi);
  }
}

/// La nouvelle version téléchargée : redémarrer maintenant, ou plus tard.
/// `installee`, elle s'ouvrira au prochain lancement ; sinon (Windows), elle
/// s'installera à la fermeture.
export function pret(fenetre, derniere, installee = true) {
  fenetre.innerHTML = `
    <strong>Krozties ${echapper(derniere)} est prêt</strong>
    <span class="sous">${installee ? "Il s'ouvrira au prochain lancement." : "Il s'installera à la fermeture de Krozties."}</span>
    <div class="actions">
      <button type="button" class="relancer">Redémarrer maintenant</button>
      <button type="button" class="plus-tard">Plus tard</button>
    </div>`;
  fenetre.querySelector('.relancer').addEventListener('click', () => {
    appeler('redemarrer').catch((e) => echec(fenetre, derniere, e.message));
  });
  fenetre.querySelector('.plus-tard').addEventListener('click', () => fenetre.remove());
}

/// Une mise à jour qui n'aboutit pas : réessayer, ou passer par GitHub.
export function echec(fenetre, derniere, raison) {
  fenetre.innerHTML = `
    <strong>La mise à jour n'a pas abouti</strong>
    <span class="erreur">${echapper(majuscule(String(raison || 'raison inconnue')))}</span>
    <div class="actions">
      <button type="button" class="telecharger">Réessayer</button>
      <button type="button" class="lien-github">Télécharger sur GitHub</button>
    </div>`;
  fenetre.querySelector('.telecharger').addEventListener('click', () => installer(fenetre, derniere));
  fenetre.querySelector('.lien-github').addEventListener('click', () => {
    appeler('versions').catch(() => {});
    fenetre.remove();
  });
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
  if (!plusRecente(memoire.derniere, actuelle)) return;
  // Une version publiée avant la mise à jour dans l'application n'a pas de
  // `latest.json` : la fenêtre renvoie alors vers GitHub.
  let auto = false;
  try {
    auto = Boolean(await appeler('majChercher'));
  } catch (e) {
    auto = false;
  }
  afficher(memoire.derniere, actuelle, auto);
}
