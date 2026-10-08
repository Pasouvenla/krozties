// Ce que Krozties exécute dans la page de DofusBook pour lire un équipement :
// leur API ne répond qu'à un moteur de rendu servi par leur origine.
//
// ⚠️ Ce script tourne sur une page qu'on ne contrôle pas et ne reçoit aucune API
// de Tauri : sa seule sortie est une navigation vers une adresse convenue, que
// l'hôte intercepte et annule.
//
// Transmis : la classe, le niveau, les dix-sept emplacements, les
// caractéristiques investies, la forgemagie et les bonus Dofus actifs. Le bloc
// `user` de la réponse (pseudonyme, identifiant) n'est ni lu ni transmis.

(async () => {
  // L'hote injecte ce script au demarrage de CHAQUE document. Un equipement se
  // lit une fois.
  if (window.__krozties) return;
  window.__krozties = 1;

  const ID = '__ID__';
  const MARQUEUR = '__MARQUEUR__';
  const SLOTS = ['ch', 'am', 'a1', 'a2', 'ce', 'bo', 'ca', 'br', 'ar', 'fa', 'mo',
                 'd1', 'd2', 'd3', 'd4', 'd5', 'd6'];

  // Le seul canal de sortie : l'hôte lit cette adresse et annule la navigation.
  const rendre = (objet) => {
    // Pas de base64 : l'hote lit la chaine de requete, que sa bibliotheque d'URL
    // decode deja. Un encodage de plus serait un decodeur de plus a ecrire.
    location.href = MARQUEUR + encodeURIComponent(JSON.stringify(objet));
  };

  try {
    const reponse = await fetch(`/api/stuffs/dofus/public/${ID}`);
    if (!reponse.ok) {
      return rendre({ erreur: reponse.status === 404
        ? "cet équipement n'existe pas, ou il est privé : rendez-le public sur DofusBook."
        : `DofusBook a répondu ${reponse.status}.` });
    }
    const charge = await reponse.json();
    const stuff = charge.stuff;
    if (!stuff || !stuff.stuffItem) {
      return rendre({ erreur: "la réponse de DofusBook ne porte pas d'équipement." });
    }

    // Les bonus Dofus ne portent aucun paramètre dans la donnée du jeu (un effet 984
    // nu) : cette route les donne en clair.
    let boosts = [];
    const reponseBoosts = await fetch(`/api/boosts/stuff/${ID}`);
    if (reponseBoosts.ok) {
      boosts = (await reponseBoosts.json())
        .filter((b) => b.active)
        .map((b) => ({
          name: b.boostName,
          stat: b.effectName,
          percent: b.effectValue * b.count,
          // Un bonus qui porte un identifiant de classe est un sort de cette
          // classe, que le solveur lance deja. Le transmettre laisse le
          // resolveur refuser de le compter deux fois.
          class_id: b.classId,
          // Le sort dont vient le bonus, premier segment de « 23841-1-3-… » : le
          // résolveur sait ainsi qu'un sort de classe, comme le Prélude au Fer, est lancé
          // par le solveur.
          effect_id: b.effectId,
        }));
    }

    // Les identifiants de DofusBook ne sont pas ceux d'Ankama : le champ `official`
    // de chaque objet porte l'identifiant Ankama.
    const officiels = Object.fromEntries((charge.items || []).map((i) => [i.id, i.official]));

    // Les variables de leur page, pas ses totaux : Krozties refait le calcul avec
    // leurs règles. Les effets d'arme (« D ») et les effets spéciaux (« O ») ne
    // font pas de statistique et restent ici.
    const fiche = {
      niveau: stuff.character_level,
      carac: stuff.stuffCarac,
      emplacements: stuff.stuffItem,
      fm: charge.fmItems || {},
      fm_global: charge.fmGlobal || {},
      objets: (charge.items || []).map((i) => ({
        id: i.id,
        nom: i.name,
        effets: (i.effects || []).filter((e) => e.type === 'E').map((e) => [e.name, e.min, e.max]),
      })),
      panoplies: (charge.cloths || []).map((c) => ({
        nom: c.name,
        nombre: c.count_item,
        effets: (c.effects || []).filter((e) => e.type === 'E').map((e) => [e.name, e.count, e.value]),
      })),
    };

    rendre({
      dofusbook: fiche,
      build: {
        v: 1,
        src: 'dofusbook',
        id: stuff.id,
        short: stuff.short_url,
        class: stuff.character_class,
        level: stuff.character_level,
        items: SLOTS.map((slot) => officiels[stuff.stuffItem[slot]] || 0),
        carac: stuff.stuffCarac,
        fm: charge.fmItems,
        fmGlobal: charge.fmGlobal,
        // La forgemagie élémentaire de l'arme, « df-85 » : sans elle, une arme
        // forgemagée frapperait dans son élément d'origine.
        fmWeapon: charge.fmWeapon,
        fmStealWeapon: charge.fmStealWeapon,
        boosts,
      },
      nom: stuff.name,
    });
  } catch (erreur) {
    rendre({ erreur: `la requête a échoué (${erreur}).` });
  }
})();
