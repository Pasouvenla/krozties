// Le pont vers le moteur. La même interface tourne dans l'application de
// bureau, où elle parle à Tauri par `invoke`, et dans un navigateur en
// développement, où elle parle au serveur HTTP du CLI : seul le transport
// change.

const DANS_TAURI = typeof window !== 'undefined'
  && window.__TAURI__ && window.__TAURI__.core;

/// La table des équivalences : un nom d'appel, une commande Tauri, une route
/// HTTP. Écrite une fois ici plutôt qu'à chaque appel.
const ROUTES = {
  classes: { commande: 'classes', chemin: '/api/classes', methode: 'GET' },
  resoudre: { commande: 'resoudre_build', chemin: '/api/resolve', argument: 'build' },
  rotation: { commande: 'rotation', chemin: '/api/solve', argument: 'requete' },
  avancement: { commande: 'avancement', chemin: '/api/progress', methode: 'GET' },
  annuler: { commande: 'annuler', chemin: '/api/cancel' },
  zones: { commande: 'zones', chemin: '/api/zone', argument: 'requete' },
  reseau: { commande: 'reseau_de_pieges', chemin: '/api/reseau', argument: 'requete' },
  portails: { commande: 'portails_eliotrope', chemin: '/api/portails', argument: 'requete' },
  bombes: { commande: 'bombes_du_roublard', chemin: '/api/bombes', argument: 'requete' },
  invocations: { commande: 'invocations_du_build', chemin: '/api/invocations', argument: 'build' },
  conseil: { commande: 'conseil_par_tour', chemin: '/api/conseil', argument: 'demande' },
  cartes: { commande: 'cartes_de_boss', chemin: '/api/cartes', methode: 'GET' },
  beta: { commande: 'beta_en_cours', chemin: '/api/beta', methode: 'GET' },
  vue: { commande: 'ligne_de_vue', chemin: '/api/vue', argument: 'requete' },
  /// Bureau seulement : elle lit un équipement par une vue invisible sur la page de
  /// DofusBook, seule origine que leur API accepte. Le serveur du CLI la refuse
  /// clairement.
  importer: { commande: 'importer_equipement', bureau_seulement: true },
};

export async function appeler(nom, charge) {
  const route = ROUTES[nom];
  if (!route) throw new Error(`appel inconnu : ${nom}`);

  if (route.bureau_seulement && !DANS_TAURI) {
    throw new Error("cette lecture n'est possible que dans l'application de bureau : "
      + "un navigateur ne peut pas interroger DofusBook depuis une autre origine.");
  }

  if (DANS_TAURI) {
    // Le moteur rend du JSON déjà sérialisé, le même des deux côtés. Sans nom
    // d'argument déclaré, la charge est la liste des arguments.
    const args = route.argument ? { [route.argument]: charge } : (charge || {});
    // Tauri rejette avec une chaîne nue, pas avec une erreur : le texte se lit sous
    // les deux formes.
    let texte;
    try {
      texte = await window.__TAURI__.core.invoke(route.commande, args);
    } catch (e) {
      throw e instanceof Error ? e : new Error(String(e));
    }
    return JSON.parse(texte);
  }

  const options = route.methode === 'GET'
    ? {}
    : { method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(charge) };
  const r = await fetch(route.chemin, options);
  const texte = await r.text();
  if (!r.ok) {
    let message = texte;
    try { message = JSON.parse(texte).error || texte; } catch (e) { /* texte brut */ }
    throw new Error(message);
  }
  return JSON.parse(texte);
}

export const hote = DANS_TAURI ? 'bureau' : 'navigateur';
