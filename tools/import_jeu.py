#!/usr/bin/env python3
"""Les sorts des classes, relus dans les fichiers du jeu.

Lit `spells.json` et `spell_levels.json` d'une version du jeu extraite par
dofusdude (depot `dofus3-main`, une release par version) et refait les niveaux
de chaque sort de `data/snapshots/breed-N.json` : couts, portees, lancers,
relances, regles de lancement, lignes de degats, autres effets, pieges et
glyphes poses. Le reste de chaque fiche (noms, descriptions, variantes,
version du releve) est garde tel quel.

    python3 tools/import_jeu.py <dossier de la version>
    python3 tools/import_jeu.py <dossier de la version> --ecrire

Sans `--ecrire`, rien n'est ecrit : la sortie dit chaque valeur qui changerait.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Effet du jeu -> (element, nature) des lignes de degats : 91 a 95 volent de la
# vie, 96 a 100 frappent.
ELEMENTS = {
    91: ("water", "life_steal"),
    92: ("earth", "life_steal"),
    93: ("air", "life_steal"),
    94: ("fire", "life_steal"),
    95: ("neutral", "life_steal"),
    96: ("water", "damage"),
    97: ("earth", "damage"),
    98: ("air", "damage"),
    99: ("fire", "damage"),
    100: ("neutral", "damage"),
}

# Les effets qui posent au sol un sous-sort : `diceNum` le nomme, `diceSide`
# dit son grade. Une rune (2022) reste dehors : elle ne frappe pas d'elle-meme.
POSES = {
    400: "piege",
    401: "glyphe de debut de tour",
    402: "glyphe de fin de tour",
    1091: "glyphe-aura",
    1165: "glyphe",
}

# Les regles de lancement d'un niveau, rangees par le jeu dans les bits de
# `m_flags`.
REGLES = (
    ("in_line", 0),
    ("in_diagonal", 1),
    ("needs_line_of_sight", 2),
    ("range_boostable", 6),
    ("needs_free_cell", 3),
    ("needs_taken_cell", 4),
    ("needs_cell_without_portal", 11),
    ("needs_free_trap_cell", 5),
)


def tableau(valeur) -> list:
    """Une liste du jeu, rangee sous `Array`."""
    if isinstance(valeur, dict):
        return valeur.get("Array") or []
    return valeur or []


def references(chemin: pathlib.Path) -> list[dict]:
    with open(chemin, encoding="utf-8") as f:
        return [r["data"] for r in json.load(f)["references"]["RefIds"]]


class Jeu:
    """Les sorts et leurs niveaux d'une version du jeu."""

    def __init__(self, dossier: pathlib.Path):
        self.sorts = {s["id"]: s for s in references(dossier / "spells.json")}
        self.niveaux = {n["id"]: n for n in references(dossier / "spell_levels.json")}
        self.par_grade = {(n["spellId"], n["grade"]): n for n in self.niveaux.values()}

    def niveaux_du_sort(self, sort: dict) -> list[dict]:
        return [self.niveaux[i] for i in tableau(sort.get("spellLevels")) if i in self.niveaux]


def zone_cells(forme: str, r: int, p2: int = 0) -> int | None:
    """Combien de cases une zone couvre ; None pour une forme non comptee."""
    r, p2 = max(r or 0, 0), max(p2 or 0, 0)
    if forme == "P":
        return 1
    if forme == "C":
        return 1 + 2 * r * (r + 1)
    if forme in ("X", "+"):
        return 1 + 4 * r
    if forme in ("-", "L"):
        return max(r, 1)
    if forme in ("T", "U"):
        return 1 + 2 * r
    if forme == "O":
        return max(4 * r, 1)
    if forme == "G":
        return (2 * r + 1) ** 2
    if forme == "W":
        return (2 * r + 1) ** 2 - 4 if r else 1
    if forme == "R":
        return (2 * r + 1) * (p2 + 1)
    if forme == "V":
        return (r + 1) ** 2
    if forme == "*":
        return 8 * r + 1
    if forme == "F":
        return 3 * (r + 1) + 1
    if forme == "Q":
        # Une croix en diagonale de `p2` a `r` cases de l'impact, l'impact
        # compris quand `p2` vaut zero.
        return 4 * max(r - max(p2, 1) + 1, 0) + (1 if p2 == 0 else 0)
    # `l`, du lanceur a la case visee, se borne a la portee du sort.
    return None


def zone(effet: dict, portee: int | None = None) -> dict | None:
    """La zone d'un effet ; une ligne depuis le lanceur couvre `portee` cases."""
    z = effet.get("zoneDescr") or {}
    if not z.get("shape"):
        return None
    forme = chr(z["shape"])
    cases = zone_cells(forme, z.get("param1") or 0, z.get("param2") or 0)
    if forme == "l" and cases is None and portee is not None:
        cases = max(portee, 1)
    return {
        "shape": forme,
        "size": z.get("param1"),
        "size2": z.get("param2") or 0,
        "cells": cases,
        "falloff_percent": z.get("damageDecreaseStepPercent") or 0,
        "falloff_steps": z.get("maxDamageDecreaseApplyCount") or 0,
    }


def zone_de_ligne(effet: dict, portee: int | None) -> dict | None:
    """La zone d'une ligne de degats du sort, `size2` en dernier comme dans les
    fiches."""
    z = zone(effet, portee)
    if z is None:
        return None
    return {k: z[k] for k in ("shape", "size", "cells", "falloff_percent", "falloff_steps", "size2")}


def lignes(effets: list[dict], portee: int | None, zone_de=zone_de_ligne) -> list[dict]:
    """Les lignes de degats : de `diceNum` a `diceSide`, fixe si `diceSide` vaut
    zero."""
    out = []
    for e in effets:
        if e["actionId"] not in ELEMENTS:
            continue
        element, nature = ELEMENTS[e["actionId"]]
        out.append({
            "element": element,
            "kind": nature,
            "range": [e["diceNum"], e["diceSide"] or e["diceNum"]],
            "order": e.get("order", 0),
            "zone": zone_de(e, portee),
            "target": e.get("targetMask"),
        })
    return out


def autres_effets(effets: list[dict], portee: int | None) -> list[dict]:
    """Les effets qui ne sont pas des lignes, valeurs comprises, et leur zone
    quand ils en ont une."""
    out = []
    for e in effets:
        if e["actionId"] in ELEMENTS:
            continue
        effet = {
            "id": e["actionId"],
            "value": e.get("value"),
            "dice_num": e.get("diceNum"),
            "dice_side": e.get("diceSide"),
            "duration": e.get("duration"),
            "delay": e.get("delay"),
            "target": e.get("targetMask"),
        }
        z = zone(e, portee)
        if z:
            effet["zone"] = z
        out.append(effet)
    return out


def pose(jeu: Jeu, effet: dict) -> dict | None:
    """Le niveau du sous-sort qu'un effet pose au sol."""
    if effet["actionId"] not in POSES or not effet.get("diceNum"):
        return None
    return jeu.par_grade.get((effet["diceNum"], effet.get("diceSide") or 1))


def lignes_posees(jeu: Jeu, effets: list[dict], portee: int | None) -> list[dict]:
    out = []
    for e in effets:
        niveau = pose(jeu, e)
        if niveau is None:
            continue
        for ligne in lignes(tableau(niveau.get("effects")), portee, zone_de=zone):
            ligne["placed_by"] = POSES[e["actionId"]]
            ligne["subspell"] = e["diceNum"]
            out.append(ligne)
    return out


def effets_poses(jeu: Jeu, effets: list[dict]) -> list[int]:
    return sorted({x["actionId"] for e in effets if (n := pose(jeu, e)) for x in tableau(n.get("effects"))})


def detail_des_poses(jeu: Jeu, brut: dict) -> tuple[dict | None, list[dict]]:
    """La zone qui declenche le piege ou le glyphe pose, et les effets du
    sous-sort, valeurs comprises."""
    tous = tableau(brut.get("effects")) + tableau(brut.get("criticalEffect"))
    declenchement, detail = None, []
    for e in tous:
        if e["actionId"] not in POSES or not e.get("diceNum"):
            continue
        premier = next(x for x in tous if x["actionId"] == e["actionId"] and x.get("diceNum") == e["diceNum"])
        declenchement = zone(premier) or declenchement
        niveau = pose(jeu, e)
        if niveau is None:
            continue
        detail.extend({
            "id": x["actionId"],
            "value": x.get("value"),
            "dice_num": x.get("diceNum"),
            "dice_side": x.get("diceSide"),
            "duration": x.get("duration"),
            "target": x.get("targetMask"),
            "zone": zone(x),
        } for x in tableau(niveau.get("effects")))
    return declenchement, detail


def pose_quelque_chose(niveaux: list[dict]) -> bool:
    return any(e["actionId"] in POSES and e.get("diceNum")
               for n in niveaux for e in tableau(n.get("effects")) + tableau(n.get("criticalEffect")))


def niveau(jeu: Jeu, brut: dict, detaille: bool) -> dict:
    """Un niveau de sort tel que les fiches le rangent."""
    portee = brut.get("range")
    normaux, critiques = tableau(brut.get("effects")), tableau(brut.get("criticalEffect"))
    out = {
        "grade": brut.get("grade"),
        "ap_cost": brut.get("apCost"),
        "max_cast_per_turn": brut.get("maxCastPerTurn"),
        "max_cast_per_target": brut.get("maxCastPerTarget"),
        "min_cast_interval": brut.get("minCastInterval"),
        "global_cooldown": brut.get("globalCooldown"),
        "initial_cooldown": brut.get("initialCooldown"),
        "max_stack": brut.get("maxStack"),
        "base_crit_percent": brut.get("criticalHitProbability"),
        "range": [brut.get("minRange"), portee],
        "cast": {nom: bool(brut.get("m_flags", 0) >> bit & 1) for nom, bit in REGLES},
        "states_criterion": brut.get("statesCriterion"),
        "normal_lines": lignes(normaux, portee),
        "critical_lines": lignes(critiques, portee),
        # Tous les effets du niveau, ceux du coup critique quand il en a.
        "effect_ids": sorted({e["actionId"] for e in (critiques or normaux)}),
        "other_effects": autres_effets(normaux, portee),
        "critical_other_effects": autres_effets(critiques, portee),
        "placed_lines": lignes_posees(jeu, normaux, portee),
        "placed_critical_lines": lignes_posees(jeu, critiques, portee),
        "placed_effect_ids": effets_poses(jeu, normaux),
    }
    if detaille:
        out["placed_trigger_zone"], out["placed_effects"] = detail_des_poses(jeu, brut)
    return out


def refaire(fiche: dict, jeu: Jeu) -> dict:
    """La fiche d'une classe, ses niveaux de sorts relus dans le jeu."""
    sorts = {s["id"]: jeu.sorts.get(s["id"]) for s in fiche["spells"]}
    manquants = [i for i, s in sorts.items() if s is None]
    if manquants:
        raise ValueError(f"sorts absents du jeu : {manquants}")
    detaille = pose_quelque_chose([n for s in sorts.values() for n in jeu.niveaux_du_sort(s)])
    out = dict(fiche)
    out["spells"] = []
    for s in fiche["spells"]:
        brut = sorts[s["id"]]
        entree = dict(s)
        entree["icon_id"] = brut.get("iconId")
        entree["levels"] = [niveau(jeu, n, detaille) for n in jeu.niveaux_du_sort(brut)]
        out["spells"].append(entree)
    return out


def ecarts(avant, apres, chemin: str = "") -> list[tuple[str, object, object]]:
    """Chaque valeur qui differe entre deux fiches."""
    if isinstance(avant, dict) and isinstance(apres, dict):
        out = []
        for cle in list(avant) + [c for c in apres if c not in avant]:
            out += ecarts(avant.get(cle), apres.get(cle), f"{chemin}.{cle}" if chemin else cle)
        return out
    if isinstance(avant, list) and isinstance(apres, list) and len(avant) == len(apres):
        return [e for i, (a, b) in enumerate(zip(avant, apres)) for e in ecarts(a, b, f"{chemin}[{i}]")]
    return [] if avant == apres else [(chemin, avant, apres)]


def lisible(fiche: dict, chemin: str) -> str:
    """Un chemin de fiche avec le nom du sort et le grade."""
    if not chemin.startswith("spells["):
        return chemin
    i = int(chemin[7:chemin.index("]")])
    sort = fiche["spells"][i]
    reste = chemin[chemin.index("]") + 2:]
    if reste.startswith("levels["):
        j = int(reste[7:reste.index("]")])
        grade = sort["levels"][j].get("grade") if j < len(sort["levels"]) else j + 1
        reste = f"grade {grade} {reste[reste.index(']') + 2:]}"
    return f"{sort['name']['fr']} ({sort['id']}) {reste}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("dossier", type=pathlib.Path, help="les fichiers d'une version du jeu")
    parser.add_argument("--ecrire", action="store_true", help="recrire les fiches")
    args = parser.parse_args()
    jeu = Jeu(args.dossier)
    total = 0
    for chemin in sorted((ROOT / "data" / "snapshots").glob("breed-*.json"), key=lambda p: int(p.stem[6:])):
        fiche = json.loads(chemin.read_text(encoding="utf-8"))
        neuve = refaire(fiche, jeu)
        trouves = ecarts(fiche, neuve)
        total += len(trouves)
        for ou, avant, apres in trouves:
            print(f"{fiche['breed']['name']} : {lisible(fiche, ou)} : {json.dumps(avant, ensure_ascii=False)}"
                  f" -> {json.dumps(apres, ensure_ascii=False)}")
        if args.ecrire and trouves:
            chemin.write_text(json.dumps(neuve, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{total} valeur(s) {'recrite(s)' if args.ecrire else 'a changer'}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
