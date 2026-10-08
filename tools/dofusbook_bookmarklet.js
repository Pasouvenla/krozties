// Build importer for DofusBook, as a bookmarklet.
//
// The whole code fits in the javascript: URL: dofusbook.net is served over https,
// and a page cannot load a script from http://127.0.0.1. Their /api/* routes
// only answer requests made from one of their own loaded pages, so the code
// runs inside their page; navigating to the local server afterwards is fine.
//
// It sends only what the solver needs: class, level, the seventeen slots, the
// invested characteristics, the forgemagic per slot and the active Dofus
// bonuses. Item stats are resolved from the vendored catalogue. The `user`
// block of the response (pseudonym, identifier) is never read nor sent.

(async () => {
  const TARGET = 'https://example.invalid/import'; // replaced at build time
  const SLOTS = ['ch', 'am', 'a1', 'a2', 'ce', 'bo', 'ca', 'br', 'ar', 'fa', 'mo',
                 'd1', 'd2', 'd3', 'd4', 'd5', 'd6'];

  const fail = (message) => {
    alert('Import impossible : ' + message);
  };

  const match = location.pathname.match(/equipement\/(\d+)/);
  if (!match) {
    return fail("ouvrez d'abord la page d'un équipement DofusBook.");
  }

  let payload;
  let boosts = [];
  try {
    const response = await fetch(`/api/stuffs/dofus/public/${match[1]}`);
    if (!response.ok) {
      return fail(`DofusBook a répondu ${response.status}. Si l'équipement est privé, rendez-le public.`);
    }
    payload = await response.json();
    // Dofus bonuses carry no parameters in the game data (a bare effect 984): this
    // endpoint has them.
    const boostResponse = await fetch(`/api/boosts/stuff/${match[1]}`);
    if (boostResponse.ok) {
      boosts = (await boostResponse.json())
        .filter((b) => b.active)
        .map((b) => ({
          name: b.boostName,
          stat: b.effectName,
          percent: b.effectValue * b.count,
          // A boost carrying a class id is one of that class's own spells,
          // which the solver already casts. Sending it lets the resolver refuse
          // to count it twice.
          class_id: b.classId,
        }));
    }
  } catch (error) {
    return fail('la requête a échoué (' + error + ').');
  }

  const stuff = payload.stuff;
  // DofusBook's item ids are their own: the `official` field on each item is the
  // Ankama id.
  const official = Object.fromEntries((payload.items || []).map((i) => [i.id, i.official]));
  const build = {
    v: 1,
    src: 'dofusbook',
    id: stuff.id,
    short: stuff.short_url,
    class: stuff.character_class,
    level: stuff.character_level,
    items: SLOTS.map((slot) => official[stuff.stuffItem[slot]] || 0),
    carac: stuff.stuffCarac,
    fm: payload.fmItems,
    fmGlobal: payload.fmGlobal,
    // La forgemagie élémentaire de l'arme, « df-85 » : sans elle, une arme
    // forgemagée frapperait dans son élément d'origine.
    fmWeapon: payload.fmWeapon,
    fmStealWeapon: payload.fmStealWeapon,
    boosts,
  };

  const encoded = btoa(unescape(encodeURIComponent(JSON.stringify(build))));
  const target = `${TARGET}#b=${encodeURIComponent(encoded)}`;

  // A navigation from https to http://127.0.0.1 is allowed. If the popup blocker
  // refuses the new tab, navigate this one instead.
  const opened = window.open(target, '_blank', 'noopener');
  if (!opened) {
    location.href = target;
  }
})();
