// A stand-in for the desktop app under the companion's page: every command is
// recorded and answers null, and the page's errors are kept.
window.appels = [];
window.erreurs = [];
window.addEventListener('error', (event) => window.erreurs.push(String(event.message)));
window.__TAURI__ = {
  core: {
    invoke: async (commande, args) => {
      window.appels.push({ commande, args });
      return 'null';
    },
  },
};
