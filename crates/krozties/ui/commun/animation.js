// Ouvrir et replier sans à-coup : la hauteur glisse en 0,28 s et le contenu
// apparaît en fondu, en descendant de quelques pixels. « Réduire les
// animations », dans le système, rend tout instantané.

const DUREE = 280;
const COURBE = 'cubic-bezier(.4, 0, .2, 1)';
const DECALAGE = 6;

const reduire = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/// Une animation qui rend la main à `fin` même si la fenêtre, masquée, la
/// suspend : l'état final arrive toujours, sans hauteur retenue.
function jouer(el, images, duree, fin, garder = false) {
  const anim = el.animate(images, { duration: duree, easing: COURBE, fill: garder ? 'forwards' : 'none' });
  let fait = false;
  const terminer = () => {
    if (fait) return;
    fait = true;
    if (fin) fin();
  };
  anim.onfinish = terminer;
  // Suspendue, l'animation retiendrait sa hauteur de départ : on l'annule.
  setTimeout(() => {
    if (fait) return;
    anim.cancel();
    terminer();
  }, duree + 80);
  return anim;
}

/// Le contenu d'un bloc, hors de sa barre de titre.
const contenuDe = (bloc, titre) => [...bloc.children].filter((e) => e !== titre);

/// Le fondu du contenu. À la fermeture, il reste effacé jusqu'à ce que le
/// bloc se ferme : la fonction rendue le libère alors.
function fondu(elements, ouvrir) {
  const anims = elements.map((e) => jouer(e, ouvrir
    ? [{ opacity: 0, transform: `translateY(-${DECALAGE}px)` }, { opacity: 1, transform: 'none' }]
    : [{ opacity: 1 }, { opacity: 0 }], DUREE, null, !ouvrir));
  return () => anims.forEach((a) => a.cancel());
}

/// La hauteur que prend `bloc` dans l'état que pose `poser`, mesurée sans
/// rien peindre.
function hauteurApres(bloc, poser, retablir) {
  poser();
  const h = bloc.offsetHeight;
  retablir();
  return h;
}

/// Un encart à bandeau, replié par la classe `repliee`.
export function animerCarte(carte, replier, fin) {
  const titre = carte.querySelector(':scope > .titre-repli');
  if (reduire() || !carte.animate) {
    carte.classList.toggle('repliee', replier);
    if (fin) fin();
    return;
  }
  const avant = carte.offsetHeight;
  if (replier) {
    const apres = hauteurApres(carte, () => carte.classList.add('repliee'), () => carte.classList.remove('repliee'));
    const liberer = fondu(contenuDe(carte, titre), false);
    jouer(carte, [{ height: `${avant}px` }, { height: `${apres}px` }], DUREE, () => {
      carte.classList.add('repliee');
      liberer();
      if (fin) fin();
    });
  } else {
    carte.classList.remove('repliee');
    const apres = carte.offsetHeight;
    fondu(contenuDe(carte, titre), true);
    jouer(carte, [{ height: `${avant}px` }, { height: `${apres}px` }], DUREE, fin);
  }
}

/// Un bloc `<details>`. Un menu dont le panneau flotte hors du flux (position
/// absolue) glisse et apparaît sans changer de hauteur.
export function animerDetails(details) {
  const resume = details.querySelector(':scope > summary');
  const ouvrir = !details.open;
  if (reduire() || !details.animate) {
    details.open = ouvrir;
    return;
  }
  const contenu = contenuDe(details, resume);
  const flottant = contenu.length && contenu.every((e) => getComputedStyle(e).position === 'absolute');
  if (flottant) {
    if (ouvrir) {
      details.open = true;
      fondu(contenu, true);
    } else {
      const liberer = fondu(contenu, false);
      setTimeout(() => {
        details.open = false;
        liberer();
      }, DUREE);
    }
    return;
  }
  const avant = details.offsetHeight;
  const deborde = details.style.overflow;
  details.style.overflow = 'hidden';
  const remettre = () => { details.style.overflow = deborde; };
  if (ouvrir) {
    details.open = true;
    const apres = details.offsetHeight;
    fondu(contenu, true);
    jouer(details, [{ height: `${avant}px` }, { height: `${apres}px` }], DUREE, remettre);
  } else {
    const apres = hauteurApres(details, () => { details.open = false; }, () => { details.open = true; });
    const liberer = fondu(contenu, false);
    jouer(details, [{ height: `${avant}px` }, { height: `${apres}px` }], DUREE, () => {
      details.open = false;
      liberer();
      remettre();
    });
  }
}

/// Tous les `<details>` de la page s'ouvrent et se replient en glissant :
/// branché une fois, par délégation, il vaut aussi pour ceux qui se dessinent
/// plus tard.
export function brancherDetails(racine) {
  racine.addEventListener('click', (e) => {
    const resume = e.target.closest('summary');
    if (!resume || !resume.parentElement || resume.parentElement.tagName !== 'DETAILS') return;
    if (e.defaultPrevented) return;
    e.preventDefault();
    animerDetails(resume.parentElement);
  });
}
