#!/usr/bin/env python3
"""Le catalogue des objets, reconstruit sans reseau depuis le releve fige.

`data/snapshots/items-effets.json` garde chaque effet de chaque objet ; les
changements des notes de sortie (`data/objets-des-notes.json`, ecrit par
`tools/note_objets.py`) s'appliquent par-dessus. Ecrit
`data/snapshots/items.json`.

    python3 tools/import_items.py
"""

from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# effectId -> the name this project uses. Only what feeds the damage pipeline;
# everything else on an item is carried through unnamed so nothing is lost.
#
# 112 is "Dommages" across ALL elements, added to each element exactly once:
# double-counting it against the per-element figures overstates a four-element
# spell by about 6%.
STATS = {
    111: "ap",
    112: "damage_all",
    115: "critical_rate",
    118: "strength",
    119: "agility",
    123: "chance",
    126: "intelligence",
    128: "mp",
    138: "power",
    418: "critical_damage",
    422: "damage_earth",
    424: "damage_fire",
    426: "damage_water",
    428: "damage_air",
    430: "damage_neutral",
    # Not used by the damage pipeline, but carried so an item that contributes
    # something is never reported as contributing nothing.
    117: "range",
    124: "wisdom",
    125: "vitality",
    174: "initiative",
    176: "prospecting",
    210: "resist_earth",
    211: "resist_water",
    212: "resist_air",
    213: "resist_fire",
    214: "resist_neutral",
    752: "flee",
    753: "tackle",
    # Les quatre pourcentages de contexte de lancer. Multiplicatifs entre eux et
    # avec les dommages finaux, l'ensemble arrondi UNE seule fois. Chacun a un
    # jumeau negatif, plus bas.
    2800: "percent_melee",
    2804: "percent_ranged",
    2808: "percent_weapon",
    2812: "percent_spell",
    2414: "percent_push",
    # Les resistances de domaine, miroir exact des quatre precedentes.
    2803: "resist_percent_melee",
    2807: "resist_percent_ranged",
    2811: "resist_percent_weapon",
    2815: "resist_percent_spell",
    1171: "percent_final",
    # Do Poussée fixe et ses résistances.
    #
    # ⚠️ 414 est le bonus, 415 le malus (texte officiel de 415 : « -#1…
    # Dommage Poussée »).
    414: "push_damage",
    416: "push_resist",
    420: "critical_resist",
    # Le reste des caracteristiques et des resistances des objets : hors du
    # calcul des degats pour la plupart, mais lues, et visibles sur la fiche.
    158: "pods",
    160: "ap_dodge",
    161: "mp_dodge",
    178: "heals",
    182: "summons",
    220: "damage_reflect",
    225: "trap_damage",
    226: "trap_power",
    240: "resist_fixed_earth",
    241: "resist_fixed_water",
    242: "resist_fixed_air",
    243: "resist_fixed_fire",
    244: "resist_fixed_neutral",
    410: "ap_reduction",
    412: "mp_reduction",
}


# Les formes negatives : chaque malus a son propre effectId (« -5% Dommages
# melee »), rattache ici a la statistique qu'il baisse.
NEGATIVE_STATS = {
    2801: "percent_melee",
    2805: "percent_ranged",
    2809: "percent_weapon",
    2813: "percent_spell",
    2415: "percent_push",
    2802: "resist_percent_melee",
    2806: "resist_percent_ranged",
    2810: "resist_percent_weapon",
    2814: "resist_percent_spell",
    1172: "percent_final",
    417: "push_resist",
    421: "critical_resist",
    415: "push_damage",
    # Les malus de caracteristique : -Force, -Vitalite, -% Critique, -Portee…
    116: "range",
    152: "chance",
    153: "vitality",
    154: "agility",
    155: "intelligence",
    156: "wisdom",
    157: "strength",
    159: "pods",
    162: "ap_dodge",
    163: "mp_dodge",
    168: "ap",
    169: "mp",
    171: "critical_rate",
    175: "initiative",
    177: "prospecting",
    179: "heals",
    215: "resist_earth",
    216: "resist_water",
    217: "resist_air",
    218: "resist_fire",
    219: "resist_neutral",
    246: "resist_fixed_water",
    411: "ap_reduction",
    413: "mp_reduction",
    419: "critical_damage",
    423: "damage_earth",
    425: "damage_fire",
    427: "damage_water",
    429: "damage_air",
    431: "damage_neutral",
    754: "flee",
    755: "tackle",
    2990: "summons",
}


def named_effects(effects: list[dict] | None) -> dict:
    """Effects this project understands, keyed by name.

    Values are the item's own range. An item rolls somewhere inside it, and
    DofusBook reports the same range rather than a rolled value, so the two
    sources are directly comparable.
    """
    out = {}
    for e in effects or []:
        eid = e.get("effectId")
        name, malus = STATS.get(eid), False
        if name is None:
            name, malus = NEGATIVE_STATS.get(eid), True
        if name is None:
            continue

        low, high = e.get("from"), e.get("to")
        if low is None:
            low = 0
        # ⚠️ `to` absent ou egal a zero : l'effet est fixe et vaut `from`. Le
        # releve a `to == 0` exactement quand le de `diceSide` vaut zero, et le
        # texte officiel affiche alors une valeur seule (« -5% Dommages aux
        # sorts »). Lu comme une borne haute, tout malus fixe tomberait a zero,
        # le build prenant le meilleur jet.
        if not high:
            high = low
        if malus:
            # Le releve porte deja les malus en magnitude negative : le signe
            # est force, sans l'inverser, pour qu'un malus reste un malus quelle
            # que soit la forme de la donnee.
            low, high = -abs(low), -abs(high)
        low, high = min(low, high), max(low, high)

        # Un objet peut porter deux lignes qui retombent sur la meme statistique
        # (un bonus et un malus, ou deux sources du meme pourcentage) : elles
        # s'additionnent, la derniere lue n'ecrase pas la precedente.
        previous = out.get(name)
        out[name] = [previous[0] + low, previous[1] + high] if previous else [low, high]
    return out


# Les modificateurs de sorts des objets de classe, et le sens de chacun : « -1 de
# relance » baisse la relance, « -5 Portee minimale » baisse la portee minimale.
SPELL_MODIFIERS = {
    # ⚠️ Le « -1 PA » n'est que dans les effets POSSIBLES des objets, jamais
    # dans leurs effets.
    285: ("ap_cost", -1),
    281: ("range_max", 1),
    282: ("range_boostable", 1),
    283: ("damage", 1),
    286: ("cooldown", -1),
    287: ("critical_rate", 1),
    288: ("no_cast_in_line", 1),
    289: ("no_line_of_sight", 1),
    290: ("casts_per_turn", 1),
    291: ("casts_per_target", 1),
    293: ("base_damage", 1),
    295: ("range_min", -1),
    297: ("no_taken_cell", 1),
}


def spell_modifiers(possible: list[dict] | None) -> list[dict]:
    """Les modificateurs de sorts d'un objet, dans l'ordre de l'objet.

    ⚠️ LA VALEUR N'EST QUE DANS LES EFFETS POSSIBLES. Les effets de l'objet
    gardent le sort (`from`) et rien d'autre (`to` a zero) ; l'effet possible
    porte le sort dans `diceNum` et la valeur dans `value`, ce que le texte
    officiel appelle #1 et #3 (« #1 : +#3 Portee maximale »).
    """
    out = []
    for e in possible or []:
        lu = SPELL_MODIFIERS.get(e.get("effectId"))
        if lu is None:
            continue
        kind, sens = lu
        out.append({"spell": e["diceNum"], "kind": kind, "value": sens * abs(e.get("value") or 0)})
    return out


def item_spells(possible: list[dict] | None) -> list[dict]:
    """Les sorts que porte un objet, dans l'ordre de l'objet.

    ⚠️ EUX AUSSI NE SONT QUE DANS LES EFFETS POSSIBLES. L'effet 1175 est
    l'effet special (« Sort : Guerre de Positions »), le sort dans `diceNum` ;
    l'effet 722 ajoute un sort TEMPORAIRE a la barre du porteur, le sort dans
    `value`. Le grade est dans `diceSide` pour les deux.
    """
    out = []
    for e in possible or []:
        if e.get("effectId") == 1175 and e.get("diceNum"):
            out.append({"spell": e["diceNum"], "grade": e.get("diceSide") or 1, "temporary": False})
        elif e.get("effectId") == 722 and e.get("value"):
            out.append({"spell": e["value"], "grade": e.get("diceSide") or 1, "temporary": True})
    return out


# Ce que la fiche d'un objet dit de ses effets sans incidence sur le combat,
# dans les mots du texte officiel. Ceux qui portent un
# numero ou une valeur se lisent dans `item_infos`. Les effets propres a un
# exemplaire (date de reception, echangeable a une date, durabilite, proprietes
# d'une monture, victimes du Vampyre) n'y sont pas : un build ne les connait pas.
INFOS_FIXES = {
    981: "Lié au personnage",
    795: "Arme de chasse",
    2818: "Fabrication coopérative impossible",
    148: "Quelqu'un vous suit !",
    146: "Change les paroles",
    149: "Change l'apparence",
}
TITRE, ATTITUDE, TAILLE, VOL_DE_KAMAS = 724, 10, 2871, 130

_NOMS: dict | None = None


def noms_des_titres_et_attitudes() -> dict:
    """Les noms des titres et des attitudes, releves une fois."""
    global _NOMS
    if _NOMS is None:
        chemin = ROOT / "data" / "snapshots" / "titres-et-attitudes.json"
        _NOMS = json.loads(chemin.read_text()) if chemin.exists() else {"titres": {}, "attitudes": {}}
    return _NOMS


def item_infos(effets: list[dict], possibles: list[dict] | None, noms: dict) -> list[str]:
    """Les lignes de la fiche d'un objet pour ses effets hors combat, sans
    doublon : un effet est souvent a la fois dans les effets de l'objet et dans
    ses effets possibles."""
    out: list[str] = []

    def ajouter(ligne: str | None) -> None:
        if ligne and ligne not in out:
            out.append(ligne)

    for e in [*(possibles or []), *effets]:
        i = e.get("effectId")
        n = e.get("diceNum") or e.get("from")
        if i in INFOS_FIXES:
            ajouter(INFOS_FIXES[i])
        elif i == TITRE:
            formes = noms["titres"].get(str(n))
            if formes:
                ajouter("Titre : " + (formes[0] if formes[0] == formes[1] else f"{formes[0]} ou {formes[1]}"))
        elif i == ATTITUDE:
            nom = noms["attitudes"].get(str(n))
            if nom:
                ajouter(f"Attitude : {nom}")
        elif i == TAILLE and n:
            ajouter(f"Taille : -{abs(n)} %")
        elif i == VOL_DE_KAMAS and n:
            haut = abs(e.get("to") or e.get("diceSide") or 0)
            ajouter(f"Vole {abs(n)} à {haut} kamas" if haut > abs(n) else f"Vole {abs(n)} kamas")
    return out


# Les lignes d'une arme (effets de categorie 2) : leur nature et leur element.
# Le vol frappe comme des degats et rend la moitie en soin ; le soin ne frappe
# pas.
WEAPON_LINES = {
    100: ("damage", "neutral"), 97: ("damage", "earth"), 99: ("damage", "fire"),
    96: ("damage", "water"), 98: ("damage", "air"), 2822: ("damage", "best"),
    95: ("steal", "neutral"), 92: ("steal", "earth"), 94: ("steal", "fire"),
    91: ("steal", "water"), 93: ("steal", "air"), 2828: ("steal", "best"),
    108: ("heal", "fire"),
}

# Ce que l'arme fait d'autre a sa cible : les cases de poussee, d'attirance ou
# d'avancee, les PA et PM retires ou voles.
WEAPON_EFFECTS = {
    5: "push", 6: "pull", 1042: "advance", 101: "ap_removal", 127: "mp_removal", 77: "mp_steal",
}

CHAMPS_ARME = ("apCost", "minRange", "range", "criticalHitProbability", "criticalHitBonus",
               "castInLine", "castInDiagonal", "castTestLos", "maxCastPerTurn", "twoHanded")


def weapon(item: dict, champs: dict | None, effets: list[dict]) -> dict | None:
    """Ce que frappe une arme et comment elle se lance, ou rien si l'objet n'en
    est pas une. `criticalHitProbability` est un taux en pour cent qui s'ajoute
    au critique du build, `criticalHitBonus` ce que gagne chaque jet sur un
    coup critique (la Hachebarde de Guerre : 15 % et +20, comme DofusBook)."""
    if not champs:
        return None
    lignes, autres = [], []
    for e in effets:
        if e.get("category") != 2:
            continue
        low, high = abs(e.get("from") or 0), abs(e.get("to") or 0)
        if not high:
            high = low
        low, high = min(low, high), max(low, high)
        if e["effectId"] in WEAPON_LINES:
            nature, element = WEAPON_LINES[e["effectId"]]
            lignes.append({"kind": nature, "element": element, "min": low, "max": high})
        elif e["effectId"] in WEAPON_EFFECTS:
            autres.append({"kind": WEAPON_EFFECTS[e["effectId"]], "min": low, "max": high})
    return {
        "type_id": item["type_id"],
        "ap": champs["apCost"],
        "range": [champs["minRange"], champs["range"]],
        "crit_rate": champs["criticalHitProbability"],
        "crit_bonus": champs["criticalHitBonus"],
        "in_line": bool(champs["castInLine"]),
        "in_diagonal": bool(champs["castInDiagonal"]),
        "line_of_sight": bool(champs["castTestLos"]),
        "casts_per_turn": champs["maxCastPerTurn"],
        "two_handed": bool(champs["twoHanded"]),
        "lines": lignes,
        "effects": autres,
    }


def entree_du_catalogue(item: dict, effets: list[dict], possibles: list[dict] | None,
                        champs_arme: dict | None = None) -> dict:
    """Ce que le catalogue garde d'un objet. Les modificateurs de sorts, les
    sorts et l'arme ne s'ecrivent que sur les objets qui en portent."""
    lus = set(STATS) | set(NEGATIVE_STATS) | set(SPELL_MODIFIERS)
    arme = weapon(item, champs_arme, effets)
    if arme:
        lus |= set(WEAPON_LINES) | set(WEAPON_EFFECTS)
    out = {
        **item,
        "stats": named_effects(effets),
        "unmapped_effect_ids": sorted({e["effectId"] for e in effets if e["effectId"] not in lus}),
    }
    modificateurs = spell_modifiers(possibles)
    if modificateurs:
        out["spell_modifiers"] = modificateurs
    sorts = item_spells(possibles)
    if sorts:
        out["spells"] = sorts
    infos = item_infos(effets, possibles, noms_des_titres_et_attitudes())
    if infos:
        out["infos"] = infos
    if arme:
        out["weapon"] = arme
    return out


NOTES_OBJETS = ROOT / "data" / "objets-des-notes.json"


def catalogue_du_releve() -> dict:
    """Le catalogue tel que le donne le releve complet des effets, sans reseau
    et sans les notes de sortie.

    `data/snapshots/items-effets.json` garde chaque effet de chaque objet : une
    correspondance d'effets corrigee ou elargie se repercute ici, sans reseau.
    """
    releve = json.loads((ROOT / "data" / "snapshots" / "items-effets.json").read_text())
    return {
        "source": releve["source"],
        "fetched_at": releve["fetched_at"],
        "game_version": releve["game_version"],
        "items": [
            entree_du_catalogue(
                {k: i[k] for k in ("id", "name", "level", "type_id", "set_id")},
                i["effects"],
                i.get("possible_effects"),
                i.get("weapon"),
            )
            for i in releve["items"]
        ],
        "sets": [
            {"id": s["id"], "name": s["name"], "bonuses": [named_effects(pas) for pas in s["effects"]]}
            for s in releve["sets"]
        ],
    }


def appliquer_les_notes(catalogue: dict, saisie: dict) -> list[str]:
    """Les changements d'objets des notes de sortie, saisis dans notre propre
    base (`data/objets-des-notes.json`, ecrit par `tools/note_objets.py`),
    par-dessus le releve fige : les objets changes ou neufs se saisissent
    depuis la note.

    Chaque valeur dit ce qu'elle remplace (`avant`, `null` pour une ligne
    absente) et `apres` a `null` retire la ligne. Le moindre ecart avec le
    releve est rendu : une saisie qui ne correspond plus a sa base ne passe
    pas en silence.
    """
    par_id = {i["id"]: i for i in catalogue["items"]}
    ecarts = []
    for note in saisie.get("notes", []):
        for ident, changement in note["objets"].items():
            objet = par_id.get(int(ident))
            if objet is None:
                ecarts.append(f"note {note['id']} : objet {ident} absent du catalogue")
                continue
            cibles = [(objet["stats"], cle, v) for cle, v in changement.get("stats", {}).items()]
            cibles += [(objet.get("weapon") or {}, cle, v) for cle, v in changement.get("arme", {}).items()]
            if "spell_modifiers" in changement:
                cibles.append((objet, "spell_modifiers", changement["spell_modifiers"]))
            for table, cle, valeur in cibles:
                if table.get(cle) != valeur["avant"]:
                    ecarts.append(f"note {note['id']} : {objet['name']} ({ident}), {cle} vaut "
                                  f"{table.get(cle)}, la saisie attend {valeur['avant']}")
                elif valeur["apres"] is None:
                    table.pop(cle, None)
                else:
                    table[cle] = valeur["apres"]
    return ecarts


def depuis_releve() -> pathlib.Path:
    """Le catalogue reconstruit depuis le releve, les notes de sortie appliquees
    par-dessus. Un ecart entre la saisie et le releve, et rien ne s'ecrit."""
    out = catalogue_du_releve()
    saisie = json.loads(NOTES_OBJETS.read_text()) if NOTES_OBJETS.exists() else {}
    ecarts = appliquer_les_notes(out, saisie)
    if ecarts:
        raise SystemExit(f"{len(ecarts)} ecart(s) entre la saisie des notes et le releve, "
                         "rien n'est ecrit :\n  " + "\n  ".join(ecarts))
    path = ROOT / "data" / "snapshots" / "items.json"
    path.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")
    return path


def main() -> int:
    path = depuis_releve()
    print(f"ecrit {path.relative_to(ROOT)} depuis le releve, notes de sortie appliquees", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
