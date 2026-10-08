#!/usr/bin/env python3
"""Ce qu'une note de sortie change aux objets, saisi dans notre propre base.

La note chiffre elle-même ce qu'elle change : les trophées par famille et par
rang (mineur / normal / majeur), les légendaires ligne à ligne, les armes en
soins Neutre par leur élément.

Le script lit la note en JSON, au format de `Note` (`crates/app/src/notes.rs`),
en tire les tableaux des trophées, y ajoute ce qui se saisit à la main
(`A_LA_MAIN`), vérifie chaque valeur d'avant contre le catalogue du relevé, puis
écrit `data/objets-des-notes.json` et la liste des objets changés dans le
manifeste. Un seul écart, et rien ne s'écrit. `tools/import_items.py` applique
ensuite la saisie par-dessus le relevé.

    python3 tools/note_objets.py <note.json>
    python3 tools/import_items.py
"""

from __future__ import annotations

import json
import pathlib
import re
import sys
import unicodedata

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

from import_items import NOTES_OBJETS, catalogue_du_releve  # noqa: E402

MANIFESTE = ROOT / "data" / "version.json"
TROPHEE = 151
RANGS = {"mineur": 0, "mineure": 0, "": 1, "majeur": 2, "majeure": 2}
ELEMENTS = (("Air", "air"), ("Eau", "water"), ("Feu", "fire"), ("Neutre", "neutral"), ("Terre", "earth"))
CINQ = tuple(en for _, en in ELEMENTS)

LISEZ_MOI = (
    "Ce que les notes de sortie changent aux objets, saisi dans notre propre base depuis la note. "
    "tools/note_objets.py l'écrit, tools/import_items.py "
    "l'applique par-dessus le relevé figé. Chaque valeur dit ce qu'elle remplace "
    "(« avant », null pour une ligne absente ; « apres » à null retire la ligne) : un écart avec "
    "le relevé arrête l'import."
)

# Le libellé de la note, et la caractéristique du catalogue qu'il nomme.
LIBELLES = {
    "% Critique": "critical_rate",
    "Dommages Critiques": "critical_damage",
    "Résistances Critiques": "critical_resist",
    "Dommages Poussée": "push_damage",
    "Résistances Poussée": "push_resist",
    "Tacle": "tackle",
    "Fuite": "flee",
    "Esquive PA": "ap_dodge",
    "Esquive PM": "mp_dodge",
    "Retrait PA": "ap_reduction",
    "Retrait PM": "mp_reduction",
    "Initiative": "initiative",
    "Pods": "pods",
    "Prospection": "prospecting",
    "Soins": "heals",
    "Force": "strength",
    "Intelligence": "intelligence",
    "Chance": "chance",
    "Agilité": "agility",
    **{f"% Résistances {fr}": f"resist_{en}" for fr, en in ELEMENTS},
    **{f"Résistances {fr}": f"resist_fixed_{en}" for fr, en in ELEMENTS},
    **{f"Dommages {fr}": f"damage_{en}" for fr, en in ELEMENTS},
}

LIGNE = re.compile(r"^(?P<malus>Malus )?(?P<libelle>.+?) \((?P<valeurs>[^)]*)\)$")

# Deux familles où la note écrit une flèche sur la seconde caractéristique
# qu'elle ajoute (« Esquive PM (4 → 5 / …) » pour le Fuyard), alors que le
# relevé 3.6.12 ne la porte pas : c'est la ligne neuve qu'annonce le résumé de
# la note, prise à sa valeur d'arrivée.
FLECHES_SUR_LIGNES_NEUVES = {"Fuyard": {"Esquive PM"}, "Insaisissable": {"Fuite"}}


def ligne(nature: str, element: str, bas: int, haut: int) -> dict:
    return {"kind": nature, "element": element, "min": bas, "max": haut}


# Ce que la note dit en prose, saisi à la main, objet par objet. Les mécanismes
# neufs se lisent ainsi :
# - « meilleur élément » : celui où le porteur frappe le plus fort, d'après les
#   caractéristiques et les dommages de son équipement (déjà lu ainsi par
#   `armes.rs` pour les armes qui le portaient) ;
# - une Puissance ajoutée « basée sur » une caractéristique vaut au plus ce que
#   dit la note (60) ;
# - des Dommages « basés sur » les Dommages d'un élément valent dans tous les
#   éléments.
# La note ne donne que le maximum d'une plage : `plafond` y ramène la plage, le
# minimum en proportion. Le minimum ne sert qu'à la fiche, le build ne lisant
# que le maximum (le meilleur jet).
A_LA_MAIN = {
    "2458873": {
        # Hachebarde de Guerre : 46 à 55 Feu deviennent 37 à 43 dans le meilleur
        # élément, plus une ligne de vol de vie de 10 à 13 ; critique +20 → +10.
        22368: [
            ("plafond", "intelligence", 60),
            ("ajout_selon", "power", 60, "intelligence"),
            ("deplace", "damage_fire", "damage_all"),
            ("arme", "crit_bonus", 10),
            ("arme", "lines", [ligne("damage", "best", 37, 43), ligne("steal", "best", 10, 13)]),
        ],
        # Plume de Buhorado (son cumul sans plafond est dans `effets_d_objets.rs`).
        20356: [
            ("plafond", "agility", 60),
            ("ajout_selon", "power", 60, "agility"),
            ("deplace", "damage_air", "damage_all"),
            ("element", "damage", "air", "best"),
        ],
        # Frisson de Brumaire.
        20355: [
            ("plafond", "chance", 60),
            ("ajout_selon", "power", 60, "chance"),
            ("deplace", "damage_water", "damage_all"),
            ("element", "damage", "water", "best"),
        ],
        # Couronne de Brâm Barbe-Monde : +10 % Critique, selon les Dommages d'armes.
        20359: [("ajout_selon", "critical_rate", 10, "percent_weapon")],
        # Étreinte de Servitude : Dommages 10 → 15, ses cinq lignes d'élément.
        22429: [("plafond", f"damage_{e}", 15) for e in CINQ],
        # Bouclier Miroir : ses malus de résistance en mêlée et de résistances
        # critiques sont retirés.
        32115: [("retire", "resist_percent_melee"), ("retire", "critical_resist")],
        # Crocobur : -30 Esquive PA et PM deviennent -20 Retrait PA et PM.
        20353: [
            ("retire", "ap_dodge"),
            ("retire", "mp_dodge"),
            ("ajout", "ap_reduction", [-20, -20]),
            ("ajout", "mp_reduction", [-20, -20]),
        ],
        # Ferveur d'Amayiro : Puissance 80 → 100, Dommages Poussée 20 → 30.
        32163: [("plafond", "power", 100), ("plafond", "push_damage", 30)],
        # Bravoure de Rykke Errel : ses Dommages fixes partent, +6 % Dommages en
        # mêlée arrivent, à leur valeur maximale.
        20361: [*(("retire", f"damage_{e}") for e in CINQ), ("ajout", "percent_melee", [6, 6])],
        # Bottes du Cul Botté : Puissance 40 → 80, +40 Résistances Poussée selon
        # leurs Résistances Neutre.
        20364: [("plafond", "power", 80), ("ajout_selon", "push_resist", 40, "resist_neutral")],
        # Ciseaux du Destin et Balance-Fléau de Misère : leur bonus aux critiques.
        20354: [("arme", "crit_bonus", 6)],
        22444: [("arme", "crit_bonus", 3)],
        # Prysantor : Résistances Critiques 30 → 50, malus de Résistances Poussée
        # 30 → 25.
        22025: [("plafond", "critical_resist", 50), ("plafond", "push_resist", -25)],
        # Amour d'Helséphine : 1 → 2 PA.
        32118: [("plafond", "ap", 2)],
        # Les armes passées de soins Feu à soins Neutre, et ce qui passe Neutre
        # avec : le vol de trois d'entre elles, les dégâts Air de l'Arc Hidsad.
        # L'Arc de Flèche Mauve perd aussi un lancer par tour (2 → 1).
        15014: [
            ("arme", "casts_per_turn", 1),
            ("element", "steal", "fire", "neutral"),
            ("element", "heal", "fire", "neutral"),
        ],
        15216: [("element", "steal", "fire", "neutral"), ("element", "heal", "fire", "neutral")],
        6508: [("element", "steal", "fire", "neutral"), ("element", "heal", "fire", "neutral")],
        1355: [("element", "damage", "air", "neutral"), ("element", "heal", "fire", "neutral")],
        **{
            arme: [("element", "heal", "fire", "neutral")]
            for arme in (25222, 11755, 26310, 6517, 6539, 8118, 27534, 7110, 17525, 7182, 6519)
        },
        # Baudrier Popée : sur Renommée, -1 tour de relance devient +1 lancer
        # par tour.
        27553: [("modificateur", 23319, ("cooldown", -1), ("casts_per_turn", 1))],
    },
}


class Ecart(Exception):
    pass


def sans_accents(texte: str) -> str:
    texte = unicodedata.normalize("NFD", texte.lower())
    return "".join(c for c in texte if unicodedata.category(c) != "Mn").strip()


def tableaux_des_trophees(noeuds: list[dict]) -> tuple[dict, dict]:
    """Les familles que la note détaille, {famille: [(libellé, malus, rangs)]}
    où chaque rang vaut (avant, après), avant à None pour une ligne neuve ; et
    les phrases sans chiffres, par famille."""
    familles: dict = {}
    phrases: dict = {}
    i = 0
    while i < len(noeuds):
        if not noeuds[i]["texte"].strip().startswith("DÉTAILS DES TROPHÉES"):
            i += 1
            continue
        profondeur, famille = noeuds[i]["profondeur"], None
        i += 1
        while i < len(noeuds) and noeuds[i]["profondeur"] > profondeur \
                and not noeuds[i]["balise"].startswith("h"):
            texte = noeuds[i]["texte"].strip()
            if noeuds[i]["profondeur"] == profondeur + 1 and texte.endswith(":"):
                famille = texte[:-1].strip()
                familles.setdefault(famille, [])
            elif famille:
                m = LIGNE.match(texte)
                if m:
                    rangs = []
                    for valeur in m["valeurs"].split("/"):
                        avant, fleche, apres = valeur.partition("→")
                        rangs.append((int(avant), int(apres)) if fleche else (None, int(avant)))
                    familles[famille].append((m["libelle"], bool(m["malus"]), rangs))
                else:
                    phrases.setdefault(famille, []).append(texte)
            i += 1
    return familles, phrases


def trophees_par_famille(catalogue: dict) -> dict:
    out: dict = {}
    for objet in catalogue["items"]:
        if objet["type_id"] != TROPHEE:
            continue
        m = re.match(r"^(.*?)(?:\s+(mineure?|majeure?))?$", objet["name"])
        out.setdefault(sans_accents(m[1]), {})[RANGS[m[2] or ""]] = objet
    return out


def changements_du_trophee(stats: dict, lignes: list, rang: int, neuves: set[str]) -> dict:
    """Les lignes d'un trophée au rang donné, vérifiées contre le relevé."""
    nommees = {LIBELLES[libelle] for libelle, _, _ in lignes}
    out = {}
    for libelle, malus, rangs in lignes:
        stat = LIBELLES[libelle]
        avant, apres = rangs[rang]
        signe = -1 if malus else 1
        neuf = [signe * apres] * 2
        actuel = stats.get(stat)
        if libelle in neuves:
            if actuel is not None:
                raise Ecart(f"{libelle} vaut {actuel}, la ligne devait être neuve")
            out[stat] = {"avant": None, "apres": neuf}
            continue
        if avant is None:
            # Une ligne neuve, ou déjà là à la même valeur.
            if actuel is None:
                out[stat] = {"avant": None, "apres": neuf}
            elif actuel != neuf:
                raise Ecart(f"{libelle} vaut {actuel}, la note le donne à {apres} sans changement")
            continue
        ancien = [signe * avant] * 2
        if actuel == ancien:
            out[stat] = {"avant": ancien, "apres": neuf}
            continue
        # Un malus qui change de caractéristique (Muraille : -2 % Résistance Feu
        # devient -150 Initiative) : l'ancien a la même ampleur, et aucune autre
        # ligne de la famille ne le nomme.
        candidats = [s for s, v in stats.items() if malus and v == [-avant, -avant] and s not in nommees]
        if actuel is None and len(candidats) == 1:
            out[candidats[0]] = {"avant": stats[candidats[0]], "apres": None}
            out[stat] = {"avant": None, "apres": neuf}
            continue
        raise Ecart(f"{libelle} vaut {actuel}, la note part de {avant}")
    return out


def plafond(plage: list[int], maximum: int) -> list[int]:
    bas, haut = plage
    if bas == haut:
        return [maximum, maximum]
    return [int(bas * maximum / haut + 0.5), maximum]


def exige(table: dict, cle: str):
    if cle not in table:
        raise Ecart(f"{cle} absent du relevé")
    return table[cle]


def saisie_a_la_main(objet: dict, operations: list) -> dict:
    """Les opérations jouées sur une copie de l'objet, rendues en avant / après."""
    stats = json.loads(json.dumps(objet["stats"]))
    arme = json.loads(json.dumps(objet.get("weapon") or {}))
    modificateurs = json.loads(json.dumps(objet.get("spell_modifiers") or []))
    for operation, *args in operations:
        if operation == "plafond":
            stat, maximum = args
            stats[stat] = plafond(exige(stats, stat), maximum)
        elif operation in ("ajout", "ajout_selon"):
            stat = args[0]
            if stat in stats:
                raise Ecart(f"{stat} déjà présent")
            if operation == "ajout":
                stats[stat] = args[1]
            else:
                maximum, base = args[1], exige(stats, args[2])
                stats[stat] = [int(maximum * base[0] / base[1] + 0.5), maximum]
        elif operation == "retire":
            exige(stats, args[0])
            del stats[args[0]]
        elif operation == "deplace":
            ancienne, neuve = args
            if neuve in stats:
                raise Ecart(f"{neuve} déjà présent")
            exige(stats, ancienne)
            stats[neuve] = stats.pop(ancienne)
        elif operation == "arme":
            champ, valeur = args
            exige(arme, champ)
            arme[champ] = valeur
        elif operation == "element":
            nature, ancien, neuf = args
            touchees = [l for l in arme.get("lines", []) if l["kind"] == nature and l["element"] == ancien]
            if not touchees:
                raise Ecart(f"aucune ligne {nature} {ancien}")
            for l in touchees:
                l["element"] = neuf
        elif operation == "modificateur":
            sort, ancien, neuf = args
            vises = [m for m in modificateurs if m["spell"] == sort and (m["kind"], m["value"]) == ancien]
            if len(vises) != 1:
                raise Ecart(f"modificateur {ancien} du sort {sort} introuvable")
            vises[0]["kind"], vises[0]["value"] = neuf
        else:
            raise Ecart(f"opération inconnue {operation}")

    out: dict = {}
    cles = list(objet["stats"]) + [k for k in stats if k not in objet["stats"]]
    changees = {k: {"avant": objet["stats"].get(k), "apres": stats.get(k)}
                for k in cles if objet["stats"].get(k) != stats.get(k)}
    if changees:
        out["stats"] = changees
    origine = objet.get("weapon") or {}
    champs = {k: {"avant": origine[k], "apres": arme[k]} for k in origine if origine[k] != arme[k]}
    if champs:
        out["arme"] = champs
    if modificateurs != (objet.get("spell_modifiers") or []):
        out["spell_modifiers"] = {"avant": objet["spell_modifiers"], "apres": modificateurs}
    return out


def ecrire_la_saisie(saisie: dict) -> None:
    """Un objet par ligne : la relecture d'une note se fait objet par objet."""
    lignes = ["{", f'  "lisez_moi": {json.dumps(LISEZ_MOI, ensure_ascii=False)},', '  "notes": [']
    for n, note in enumerate(saisie["notes"]):
        lignes.append("    {")
        for cle in ("id", "titre", "url"):
            lignes.append(f'      "{cle}": {json.dumps(note.get(cle), ensure_ascii=False)},')
        lignes.append('      "objets": {')
        objets = list(note["objets"].items())
        for o, (ident, changement) in enumerate(objets):
            virgule = "," if o < len(objets) - 1 else ""
            lignes.append(f'        "{ident}": {json.dumps(changement, ensure_ascii=False)}{virgule}')
        lignes.append("      }")
        lignes.append("    }" + ("," if n < len(saisie["notes"]) - 1 else ""))
    lignes += ["  ]", "}"]
    NOTES_OBJETS.write_text("\n".join(lignes) + "\n")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    note = json.loads(pathlib.Path(argv[1]).read_text())
    catalogue = catalogue_du_releve()
    par_id = {i["id"]: i for i in catalogue["items"]}
    familles, phrases = tableaux_des_trophees(note["noeuds"])
    index = trophees_par_famille(catalogue)

    objets: dict = {}
    ecarts: list[str] = []
    couvertes: set[str] = set()
    for famille, lignes in familles.items():
        inconnus = [libelle for libelle, _, _ in lignes if libelle not in LIBELLES]
        if inconnus:
            ecarts.append(f"{famille} : libellés inconnus {inconnus}")
            continue
        rangs = index.get(sans_accents(famille))
        if not rangs:
            ecarts.append(f"{famille} : aucune famille de ce nom au catalogue")
            continue
        couvertes.add(sans_accents(famille))
        for rang, objet in sorted(rangs.items()):
            try:
                stats = changements_du_trophee(objet["stats"], lignes, rang,
                                               FLECHES_SUR_LIGNES_NEUVES.get(famille, set()))
            except Ecart as e:
                ecarts.append(f"{objet['name']} : {e}")
                continue
            if stats:
                objets[str(objet["id"])] = {"nom": objet["name"], "stats": stats}

    for ident, operations in A_LA_MAIN.get(note["id"], {}).items():
        objet = par_id.get(ident)
        if objet is None:
            ecarts.append(f"objet {ident} absent du catalogue")
            continue
        try:
            changement = saisie_a_la_main(objet, operations)
        except Ecart as e:
            ecarts.append(f"{objet['name']} : {e}")
            continue
        objets[str(ident)] = {"nom": objet["name"], **changement}

    hors_detail = sorted(o["name"] for f, rangs in index.items() if f not in couvertes for o in rangs.values())
    trophees = sum(1 for k in objets if par_id[int(k)]["type_id"] == TROPHEE)
    print(f"{len(familles)} familles de trophées détaillées par la note, {trophees} trophées changés ; "
          f"{len(A_LA_MAIN.get(note['id'], {}))} objets saisis à la main", file=sys.stderr)
    for famille, textes in phrases.items():
        for texte in textes:
            print(f"  sans chiffres, {famille} : {texte}", file=sys.stderr)
    print(f"  {len(hors_detail)} trophées hors du détail de la note : {', '.join(hors_detail)}", file=sys.stderr)
    if ecarts:
        print(f"{len(ecarts)} écart(s), rien n'est écrit :", file=sys.stderr)
        for e in ecarts:
            print(f"  {e}", file=sys.stderr)
        return 1

    saisie = json.loads(NOTES_OBJETS.read_text()) if NOTES_OBJETS.exists() else {"notes": []}
    entree = {"id": note["id"], "titre": note.get("titre"), "url": note.get("url"),
              "objets": dict(sorted(objets.items(), key=lambda o: int(o[0])))}
    saisie["notes"] = [n for n in saisie["notes"] if n["id"] != note["id"]] + [entree]
    ecrire_la_saisie(saisie)

    manifeste = json.loads(MANIFESTE.read_text())
    for n in manifeste["patch_notes"]:
        if n["id"] == note["id"]:
            n["objets"] = sorted(int(i) for i in objets)
            restants = {k: v for k, v in (n.get("objets_a_relever") or {}).items() if k not in objets}
            if restants:
                n["objets_a_relever"] = restants
            else:
                n.pop("objets_a_relever", None)
    MANIFESTE.write_text(json.dumps(manifeste, indent=2, ensure_ascii=False) + "\n")
    print(f"écrit {NOTES_OBJETS.relative_to(ROOT)} : {len(objets)} objets ; manifeste à jour", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
