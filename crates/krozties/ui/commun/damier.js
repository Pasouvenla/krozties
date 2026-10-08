// Le damier isométrique, celui du jeu, commun à tous les outils. Il ne connaît
// ni sort ni piège : il projette, dessine des cases et écoute les clics ; le
// plateau (damier vide ou carte de boss) lui est donné.
//
// # Le repère
//
// Le serveur raisonne en (x, y), la distance étant la somme des écarts absolus.
// Ce module ne fait que projeter : une rotation de 45 degrés aplatie de moitié,
// qui fait des quatre directions du jeu les quatre diagonales de l'écran.

export const LARGEUR_CASE = 42;
export const HAUTEUR_CASE = 21;

export const versEcran = (x, y) => [
  ((x - y) * LARGEUR_CASE) / 2,
  ((x + y) * HAUTEUR_CASE) / 2,
];

export const memeCase = (a, b) => !!a && !!b && a[0] === b[0] && a[1] === b[1];

export const distance = (a, b) => Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1]);

export function losange(x, y) {
  const [cx, cy] = versEcran(x, y);
  const w = LARGEUR_CASE / 2;
  const h = HAUTEUR_CASE / 2;
  return [[cx, cy - h], [cx + w, cy], [cx, cy + h], [cx - w, cy]]
    .map((p) => p.join(',')).join(' ');
}

/// Le contour d'un ensemble de cases : les arêtes qu'une case ne partage avec
/// aucune autre case de l'ensemble. C'est ce qui distingue deux zones qui se
/// recouvrent.
export function contourZone(cases) {
  const dans = new Set(cases.map((c) => `${c[0]},${c[1]}`));
  const w = LARGEUR_CASE / 2;
  const h = HAUTEUR_CASE / 2;
  let d = '';
  cases.forEach(([x, y]) => {
    const [cx, cy] = versEcran(x, y);
    const aretes = [
      [[x + 1, y], [cx + w, cy], [cx, cy + h]],
      [[x, y + 1], [cx, cy + h], [cx - w, cy]],
      [[x - 1, y], [cx - w, cy], [cx, cy - h]],
      [[x, y - 1], [cx, cy - h], [cx + w, cy]],
    ];
    aretes.forEach(([voisine, a, b]) => {
      if (dans.has(`${voisine[0]},${voisine[1]}`)) return;
      d += `M ${a.join(',')} L ${b.join(',')} `;
    });
  });
  return d;
}

/// La hauteur d'un mur au-dessus du sol, en unités du dessin : un bloc un peu
/// moins haut qu'une case n'est large, comme les murs bas des arènes.
const HAUTEUR_MUR = 16;

/// Du fond vers l'avant : l'ordre de dessin EST la profondeur, comme en jeu.
const parProfondeur = (a, b) => (a[0] + a[1]) - (b[0] + b[1]) || a[0] - b[0];

/// Un mur en bloc : ses deux faces visibles, puis son dessus.
function bloc(x, y, classes) {
  const [cx, cy] = versEcran(x, y);
  const w = LARGEUR_CASE / 2;
  const h = HAUTEUR_CASE / 2;
  const m = HAUTEUR_MUR;
  const pts = (liste) => liste.map((q) => q.join(',')).join(' ');
  return `<g class="bloc${classes.length ? ' ' + classes.join(' ') : ''}" data-x="${x}" data-y="${y}">`
    + `<polygon class="gauche" points="${pts([[cx - w, cy - m], [cx, cy + h - m], [cx, cy + h], [cx - w, cy]])}"/>`
    + `<polygon class="droite" points="${pts([[cx, cy + h - m], [cx + w, cy - m], [cx + w, cy], [cx, cy + h]])}"/>`
    + `<polygon class="dessus" points="${pts([[cx, cy - h - m], [cx + w, cy - m], [cx, cy + h - m], [cx - w, cy - m]])}"/>`
    + '</g>';
}

/// Dessine le plateau dans `hote`.
///
/// `options.plateau` porte `sol` et `murs` (le damier vide ou une carte de boss,
/// tels que `plateaux.js` les rend) : seul le sol se clique, un mur se dessine en
/// bloc, un trou pas du tout. `options.case(x, y)` rend `{ classes, titre }` pour
/// chaque case de sol, `options.mur(x, y)` `{ classes }` pour chaque mur.
/// `options.calques` et `options.jetons` sont des fragments SVG dessinés au-dessus
/// du sol, puis au-dessus de tout ; les murs passent par-dessus les calques.
/// `options.apercu(x, y)` rend ce qu'un clic poserait sur la case survolée,
/// dessiné sous le curseur.
///
/// Les calques ne reçoivent pas la souris, pour laisser passer les clics de pose.
export function dessiner(hote, options) {
  const {
    plateau,
    margeHaut = 64,
    case: parCase = () => ({}),
    mur: parMur = () => ({}),
    calques = '',
    jetons = '',
    surClic,
    surSurvol,
    apercu,
  } = options;

  const sol = [...plateau.sol].sort(parProfondeur);
  const murs = [...plateau.murs].sort(parProfondeur);
  let [gauche, droite, haut, bas] = [Infinity, -Infinity, Infinity, -Infinity];
  [...sol, ...murs].forEach(([x, y]) => {
    const [cx, cy] = versEcran(x, y);
    gauche = Math.min(gauche, cx - LARGEUR_CASE / 2);
    droite = Math.max(droite, cx + LARGEUR_CASE / 2);
    haut = Math.min(haut, cy - HAUTEUR_CASE / 2 - (murs.length ? HAUTEUR_MUR : 0));
    bas = Math.max(bas, cy + HAUTEUR_CASE / 2);
  });

  let cases = '';
  sol.forEach(([x, y]) => {
    const { classes = [], titre = '' } = parCase(x, y) || {};
    // Une case sur deux plus sombre, comme le sol d'un combat.
    const sombre = (((x + y) % 2) + 2) % 2 === 1 ? ' sombre' : '';
    cases += `<polygon class="case${sombre}${classes.length ? ' ' + classes.join(' ') : ''}"`
      + ` points="${losange(x, y)}" tabindex="0" role="button"`
      + ` data-x="${x}" data-y="${y}">`
      + (titre ? `<title>${titre}</title>` : '')
      + '</polygon>';
  });
  const blocs = murs.map(([x, y]) => bloc(x, y, (parMur(x, y) || {}).classes || [])).join('');

  hote.innerHTML = `<svg viewBox="${gauche - 2} ${haut - margeHaut} `
    + `${droite - gauche + 4} ${bas - haut + margeHaut + 4}" role="group" aria-label="Damier">`
    + cases + calques + blocs + jetons + '<g class="apercu-pose"></g></svg>';

  // L'aperçu survit au redessin d'un clic : la case survolée est retenue sur
  // l'hôte.
  hote.apercu = apercu;
  rafraichirApercu(hote);
  hote.querySelector('svg').addEventListener('mouseleave', () => montrerApercu(hote, null));

  hote.querySelectorAll('polygon.case').forEach((el) => {
    const x = Number(el.dataset.x);
    const y = Number(el.dataset.y);
    if (surClic) {
      el.addEventListener('click', () => surClic(x, y));
      // Le damier se pilote aussi au clavier : sans ça, poser quoi que ce soit
      // n'aurait été possible qu'à la souris.
      el.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); surClic(x, y); }
      });
    }
    el.addEventListener('mouseenter', () => montrerApercu(hote, [x, y]));
    el.addEventListener('focus', () => montrerApercu(hote, [x, y]));
    el.addEventListener('blur', () => montrerApercu(hote, null));
    if (surSurvol) {
      el.addEventListener('mouseenter', () => surSurvol(x, y));
      el.addEventListener('focus', () => surSurvol(x, y));
      el.addEventListener('mouseleave', () => surSurvol(null, null));
      el.addEventListener('blur', () => surSurvol(null, null));
    }
  });
}

/// Montre l'aperçu de pose de la case `ici`, ou l'efface sans case.
function montrerApercu(hote, ici) {
  hote.survolee = ici;
  const couche = hote.querySelector('g.apercu-pose');
  if (!couche) return;
  couche.innerHTML = ici && hote.apercu ? (hote.apercu(ici[0], ici[1]) || '') : '';
}

/// Recalcule l'aperçu de la case survolée : à appeler quand l'outil actif ou
/// la pièce choisie change sans que le plateau soit redessiné.
export function rafraichirApercu(hote) {
  montrerApercu(hote, hote.survolee || null);
}

/// Pose ou retire `classe` sur les cases et les murs déjà dessinés, selon que
/// leur clé `x,y` est dans `cles`. Pour un survol : redessiner le plateau à
/// chaque mouvement de souris ferait perdre la case survolée.
export function marquer(hote, classe, cles) {
  hote.querySelectorAll('polygon.case, g.bloc').forEach((el) => {
    el.classList.toggle(classe, cles.has(`${el.dataset.x},${el.dataset.y}`));
  });
}
