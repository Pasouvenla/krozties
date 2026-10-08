#!/usr/bin/env python3
"""Signe et bornes des effets d'objets, et chaque correspondance d'effet
confrontee a son texte officiel.

Lancer : ./tools/test_import_items.py

Chaque malus a son propre effectId (« -5% Dommages aux sorts » est 2813, pas
2812) et en inverser le signe le ferait ressortir en bonus. L'effet 415 est le
malus de Dommages Poussee (« -#1… Dommage Poussée »), et une borne haute a zero
marque un effet fixe, pas une borne : tout malus fixe tomberait sinon a zero.
Le controle des textes officiels attrape, pour tout effet lu, une
correspondance prise a l'envers.
"""

import importlib.util
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("import_items", ROOT / "import_items.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
named_effects = module.named_effects

CAS = [
    (
        "malus fixe a magnitude deja negative : lu tel quel, jamais retourne",
        [{"effectId": 2813, "from": -5, "to": 0}],
        {"percent_spell": [-5, -5]},
    ),
    (
        "les six katanas : -10 % de dommages distance, fixe",
        [{"effectId": 2805, "from": -10, "to": 0}],
        {"percent_ranged": [-10, -10]},
    ),
    (
        "bonus ordinaire, inchange",
        [{"effectId": 2808, "from": 7, "to": 10}],
        {"percent_weapon": [7, 10]},
    ),
    (
        "borne haute ABSENTE : l'effet vaut sa borne basse",
        [{"effectId": 2812, "from": 5}],
        {"percent_spell": [5, 5]},
    ),
    (
        "borne haute EGALE A ZERO : l'effet est fixe",
        [{"effectId": 2801, "from": -3, "to": 0}],
        {"percent_melee": [-3, -3]},
    ),
    (
        "deux lignes sur la meme statistique : elles s'additionnent",
        [
            {"effectId": 2812, "from": 6, "to": 6},
            {"effectId": 2813, "from": -5, "to": 0},
        ],
        {"percent_spell": [1, 1]},
    ),
    (
        "un malus arrivant avec une magnitude positive reste un malus",
        [{"effectId": 2809, "from": 4, "to": 4}],
        {"percent_weapon": [-4, -4]},
    ),
    (
        "effet inconnu : ignore, pas devine",
        [{"effectId": 999999, "from": 10, "to": 10}],
        {},
    ),
    (
        "414 est le bonus de Dommages Poussee",
        [{"effectId": 414, "from": 10, "to": 15}],
        {"push_damage": [10, 15]},
    ),
    (
        "415 est le malus de Dommages Poussee",
        [{"effectId": 415, "from": -20, "to": 0}],
        {"push_damage": [-20, -20]},
    ),
    (
        "malus de caracteristique fixe : -10 Force",
        [{"effectId": 157, "from": -10, "to": 0}],
        {"strength": [-10, -10]},
    ),
    (
        "malus de caracteristique qui roule : -11 a -40 Force",
        [{"effectId": 157, "from": -11, "to": -40}],
        {"strength": [-40, -11]},
    ),
]

# Les modificateurs de sorts, lus dans les effets POSSIBLES : le sort dans
# `diceNum`, la valeur dans `value`.
CAS_SORTS = [
    (
        "Chapeau Leufere : +35 % de critique sur Lapement",
        [{"effectId": 287, "diceNum": 12873, "diceSide": 0, "value": 35}],
        [{"spell": 12873, "kind": "critical_rate", "value": 35}],
    ),
    (
        "« -1 de relance » baisse la relance",
        [{"effectId": 286, "diceNum": 13148, "diceSide": 0, "value": 1}],
        [{"spell": 13148, "kind": "cooldown", "value": -1}],
    ),
    (
        "Sangle Cible : « -5 Portee minimale » baisse la portee minimale",
        [{"effectId": 295, "diceNum": 13047, "diceSide": 0, "value": 5}],
        [{"spell": 13047, "kind": "range_min", "value": -5}],
    ),
    (
        "ligne de vue desactivee",
        [{"effectId": 289, "diceNum": 13106, "diceSide": 0, "value": 1}],
        [{"spell": 13106, "kind": "no_line_of_sight", "value": 1}],
    ),
    (
        "une caracteristique n'est pas un modificateur de sort",
        [{"effectId": 118, "diceNum": 10, "diceSide": 20, "value": 0}],
        [],
    ),
]

# Les sorts des objets, eux aussi dans les effets POSSIBLES : l'effet special
# (1175, le sort dans `diceNum`) et le sort temporaire (722, dans `value`).
CAS_SORTS_D_OBJETS = [
    (
        "Hachebarde de Guerre : son effet special, Guerre de Positions",
        [{"effectId": 1175, "diceNum": 15739, "diceSide": 1, "value": 0}],
        [{"spell": 15739, "grade": 1, "temporary": False}],
    ),
    (
        "Epee Necronyx : la Pelle Fantomatique, sort temporaire",
        [{"effectId": 722, "diceNum": 0, "diceSide": 1, "value": 2056},
         {"effectId": 981, "diceNum": 0, "diceSide": 0, "value": 0}],
        [{"spell": 2056, "grade": 1, "temporary": True}],
    ),
    (
        "un modificateur de sort n'est pas un sort d'objet",
        [{"effectId": 287, "diceNum": 12873, "diceSide": 0, "value": 35}],
        [],
    ),
]

# Les effets hors combat que montre la fiche d'un objet, sur un releve de noms
# reduit a ce que les cas citent.
NOMS = {"titres": {"17": ["Pourfendeur du Dark Vlad", "Pourfendeuse du Dark Vlad"],
                   "60": ["Commère des Miches", "Commère des Miches"]},
        "attitudes": {"186": "Dofus Pourpre"}}
CAS_INFOS = [
    (
        "Bouclier trophee du Dark Vlad : le titre, aux deux formes",
        [{"effectId": 724, "from": 17}],
        [{"effectId": 724, "diceNum": 17}],
        ["Titre : Pourfendeur du Dark Vlad ou Pourfendeuse du Dark Vlad"],
    ),
    (
        "un titre qui ne change pas avec le personnage s'ecrit une fois",
        [],
        [{"effectId": 724, "diceNum": 60}],
        ["Titre : Commère des Miches"],
    ),
    (
        "Dofus Pourpre : son attitude ; l'effet special 984 et « Echangeable » n'y sont pas",
        [{"effectId": 984}],
        [{"effectId": 10, "diceNum": 186}, {"effectId": 983, "value": 124}],
        ["Attitude : Dofus Pourpre"],
    ),
    (
        "un effet dans les effets ET les effets possibles ne se dit qu'une fois",
        [{"effectId": 981}],
        [{"effectId": 981}],
        ["Lié au personnage"],
    ),
    (
        "Baguette Rikiki et Kamapeche : les valeurs",
        [{"effectId": 2871, "from": 40}, {"effectId": 130, "from": 21, "to": 30}],
        [],
        ["Taille : -40 %", "Vole 21 à 30 kamas"],
    ),
]

# Les armes : la Hachebarde de Guerre (15 % de critique, +20, 5 PA, portee 1
# a 2, vole 2 PM).
CAS_ARMES = [
    (
        "Hachebarde de Guerre",
        {"type_id": 19},
        {"apCost": 5, "minRange": 1, "range": 2, "criticalHitProbability": 15,
         "criticalHitBonus": 20, "castInLine": True, "castInDiagonal": True,
         "castTestLos": True, "maxCastPerTurn": 1, "twoHanded": False},
        [{"effectId": 99, "category": 2, "from": 46, "to": 55},
         {"effectId": 77, "category": 2, "from": 2, "to": 0},
         {"effectId": 125, "category": 0, "from": 401, "to": 450}],
        {"type_id": 19, "ap": 5, "range": [1, 2], "crit_rate": 15, "crit_bonus": 20,
         "in_line": True, "in_diagonal": True, "line_of_sight": True, "casts_per_turn": 1,
         "two_handed": False,
         "lines": [{"kind": "damage", "element": "fire", "min": 46, "max": 55}],
         "effects": [{"kind": "mp_steal", "min": 2, "max": 2}]},
    ),
]

# Le texte officiel de chaque ligne et de chaque effet d'arme.
TEXTES_ARMES = {
    ("damage", "neutral"): "dommages Neutre", ("damage", "earth"): "dommages Terre",
    ("damage", "fire"): "dommages Feu", ("damage", "water"): "dommages Eau",
    ("damage", "air"): "dommages Air", ("damage", "best"): "dommages du meilleur élément",
    ("steal", "neutral"): "vol Neutre", ("steal", "earth"): "vol Terre",
    ("steal", "fire"): "vol Feu", ("steal", "water"): "vol Eau", ("steal", "air"): "vol Air",
    ("steal", "best"): "vol du meilleur élément", ("heal", "fire"): "soins Feu",
    "push": "Repousse de case", "pull": "Attire de case", "advance": "Avance de case",
    "ap_removal": "PA", "mp_removal": "PM", "mp_steal": "Vole PM",
}

# Le texte officiel de chaque modificateur, sans « #1 : » : son signe, puis
# ce qu'il dit sans sa valeur.
TEXTES_SORTS = {
    "range_max": ("+", "Portée maximale"), "range_min": ("-", "Portée minimale"),
    "range_boostable": ("", "Portée modifiable"), "damage": ("+", "Dommages"),
    "ap_cost": ("-", "PA"),
    "cooldown": ("-", "de relance"), "critical_rate": ("+", "% Critique"),
    "no_cast_in_line": ("", "lancer en ligne désactivé"),
    "no_line_of_sight": ("", "ligne de vue désactivée"),
    "casts_per_turn": ("+", "lancer(s) par tour"), "casts_per_target": ("+", "lancer(s) par cible"),
    "base_damage": ("+", "dégâts de base"),
    "no_taken_cell": ("", "case occupée nécessaire désactivée"),
}

# Le texte officiel que doit porter chaque nom, sans ses nombres ni son signe.
# Un effet lu sous un nom dont le texte dit autre chose est une erreur.
TEXTES = {
    "ap": "PA", "mp": "PM", "range": "Portée", "strength": "Force", "agility": "Agilité",
    "chance": "Chance", "intelligence": "Intelligence", "wisdom": "Sagesse",
    "vitality": "Vitalité", "power": "Puissance", "initiative": "Initiative",
    "prospecting": "Prospection", "critical_rate": "% Critique", "damage_all": "Dommage",
    "critical_damage": "Dommage Critiques", "critical_resist": "Résistance Critiques",
    "push_damage": "Dommage Poussée", "push_resist": "Résistance Poussée",
    "damage_earth": "Dommage Terre", "damage_fire": "Dommage Feu", "damage_water": "Dommage Eau",
    "damage_air": "Dommage Air", "damage_neutral": "Dommage Neutre",
    "resist_earth": "% Résistance Terre", "resist_fire": "% Résistance Feu",
    "resist_water": "% Résistance Eau", "resist_air": "% Résistance Air",
    "resist_neutral": "% Résistance Neutre",
    "resist_fixed_earth": "Résistance Terre", "resist_fixed_fire": "Résistance Feu",
    "resist_fixed_water": "Résistance Eau", "resist_fixed_air": "Résistance Air",
    "resist_fixed_neutral": "Résistance Neutre",
    "flee": "Fuite", "tackle": "Tacle", "pods": "Pod", "ap_dodge": "Esquive PA",
    "mp_dodge": "Esquive PM", "heals": "Soin", "summons": "Invocation",
    "damage_reflect": "Dommages Renvoyés", "trap_damage": "Dommage Pièges",
    "trap_power": "Puissance Pièges", "ap_reduction": "Retrait PA", "mp_reduction": "Retrait PM",
    "percent_melee": "% Dommages mêlée", "percent_ranged": "% Dommages distance",
    "percent_weapon": "% Dommages d'armes", "percent_spell": "% Dommages aux sorts",
    "resist_percent_melee": "% Résistance mêlée", "resist_percent_ranged": "% Résistance distance",
}


# La saisie des notes de sortie par-dessus le releve : ce qu'elle change, et le
# garde-fou qui rend tout ecart avec le releve.
def _hache():
    return {"items": [{"id": 1, "name": "Hache", "stats": {"power": [61, 80], "damage_fire": [7, 10]},
                       "weapon": {"crit_bonus": 20}}]}


CAS_NOTES = [
    ("une note change une valeur, en ajoute une, en retire une",
     _hache(),
     {"notes": [{"id": "n", "objets": {"1": {"stats": {
         "power": {"avant": [61, 80], "apres": [76, 100]},
         "damage_fire": {"avant": [7, 10], "apres": None},
         "damage_all": {"avant": None, "apres": [7, 10]}},
         "arme": {"crit_bonus": {"avant": 20, "apres": 10}}}}}]},
     {"stats": {"power": [76, 100], "damage_all": [7, 10]}, "weapon": {"crit_bonus": 10}}, 0),
    ("une saisie qui ne part plus du releve est rendue, sa valeur reste celle du releve",
     _hache(),
     {"notes": [{"id": "n", "objets": {"1": {"stats": {"power": {"avant": [41, 60], "apres": [76, 100]}}}}}]},
     {"stats": {"power": [61, 80], "damage_fire": [7, 10]}, "weapon": {"crit_bonus": 20}}, 1),
    ("un objet absent du catalogue est rendu",
     _hache(),
     {"notes": [{"id": "n", "objets": {"2": {"stats": {"power": {"avant": None, "apres": [1, 1]}}}}}]},
     {"stats": {"power": [61, 80], "damage_fire": [7, 10]}, "weapon": {"crit_bonus": 20}}, 1),
]


def texte_officiel(description: str) -> tuple[bool, str]:
    """Le texte d'un effet sans ses nombres ni ses accolades, et s'il est un malus."""
    t = re.sub(r"\{\{[^}]*\}\}", "", description or "")
    t = re.sub(r"#\d", "", t)
    t = " ".join(t.split())
    return t.startswith("-"), t.lstrip("-").strip()


def controle_des_textes() -> int:
    """Chaque effet lu, confronte au texte officiel du releve complet."""
    releve = ROOT.parent / "data" / "snapshots" / "items-effets.json"
    officiels = {e["id"]: e["description"] for e in json.loads(releve.read_text())["effects"]}
    echecs = vus = 0
    for table, malus in ((module.STATS, False), (module.NEGATIVE_STATS, True)):
        for eid, nom in sorted(table.items()):
            if eid not in officiels:
                continue
            vus += 1
            est_malus, texte = texte_officiel(officiels[eid])
            if est_malus != malus or texte != TEXTES.get(nom):
                echecs += 1
                print(f"ECHEC  {eid} lu « {nom} » {'malus' if malus else 'bonus'}, "
                      f"texte officiel « {'-' if est_malus else ''}{texte} »")
    print(f"{'ok' if not echecs else 'ECHEC'}     {vus} effets lus confrontes a leur texte officiel")
    # Les modificateurs de sorts : le texte, et le sens que l'import lui donne.
    for eid, (kind, sens) in sorted(module.SPELL_MODIFIERS.items()):
        texte = (officiels.get(eid) or "").replace("#1 :", "").strip()
        signe = texte[:1] if texte[:1] in "+-" else ""
        lu = (signe, texte.lstrip("+-").replace("#3", "").strip())
        if lu != TEXTES_SORTS.get(kind) or (sens < 0) != (signe == "-"):
            echecs += 1
            print(f"ECHEC  {eid} lu « {kind} » sens {sens}, texte officiel « {officiels.get(eid)} »")
    print(f"{'ok' if not echecs else 'ECHEC'}     {len(module.SPELL_MODIFIERS)} modificateurs de sorts "
          "confrontes a leur texte officiel")
    # Les lignes et effets d'arme : le texte, et le signe des retraits.
    for eid, cle in [*module.WEAPON_LINES.items(), *module.WEAPON_EFFECTS.items()]:
        est_malus, texte = texte_officiel(officiels.get(eid))
        if texte != TEXTES_ARMES.get(cle) or est_malus != (cle in ("ap_removal", "mp_removal")):
            echecs += 1
            print(f"ECHEC  {eid} lu « {cle} », texte officiel « {officiels.get(eid)} »")
    print(f"{'ok' if not echecs else 'ECHEC'}     "
          f"{len(module.WEAPON_LINES) + len(module.WEAPON_EFFECTS)} effets d'arme confrontes a leur texte officiel")
    # Les effets hors combat : le texte fixe est le texte officiel, et ceux qui
    # portent une valeur disent ce que dit le leur.
    for eid, texte in sorted(module.INFOS_FIXES.items()):
        if officiels.get(eid) != texte:
            echecs += 1
            print(f"ECHEC  {eid} lu « {texte} », texte officiel « {officiels.get(eid)} »")
    for eid, mot in ((module.TITRE, "Titre"), (module.ATTITUDE, "Attitude"),
                     (module.TAILLE, "Taille"), (module.VOL_DE_KAMAS, "Kamas")):
        if mot not in (officiels.get(eid) or ""):
            echecs += 1
            print(f"ECHEC  {eid} lu « {mot} », texte officiel « {officiels.get(eid)} »")
    print(f"{'ok' if not echecs else 'ECHEC'}     {len(module.INFOS_FIXES) + 4} effets hors combat "
          "confrontes a leur texte officiel")
    return echecs


def main() -> int:
    echecs = 0
    for nom, effets, attendu in CAS:
        obtenu = named_effects(effets)
        if obtenu != attendu:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}")
        else:
            print(f"ok     {nom}")
    for nom, effets, attendu in CAS_SORTS:
        obtenu = module.spell_modifiers(effets)
        if obtenu != attendu:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}")
        else:
            print(f"ok     {nom}")
    for nom, effets, attendu in CAS_SORTS_D_OBJETS:
        obtenu = module.item_spells(effets)
        if obtenu != attendu:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}")
        else:
            print(f"ok     {nom}")
    for nom, effets, possibles, attendu in CAS_INFOS:
        obtenu = module.item_infos(effets, possibles, NOMS)
        if obtenu != attendu:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}")
        else:
            print(f"ok     {nom}")
    for nom, item, champs, effets, attendu in CAS_ARMES:
        obtenu = module.weapon(item, champs, effets)
        if obtenu != attendu:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}")
        else:
            print(f"ok     {nom}")
    for nom, catalogue, saisie, attendu, nb_ecarts in CAS_NOTES:
        ecarts = module.appliquer_les_notes(catalogue, saisie)
        objet = {k: catalogue["items"][0][k] for k in ("stats", "weapon")}
        if objet != attendu or len(ecarts) != nb_ecarts:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {objet} {ecarts}\n       attendu  {attendu}, {nb_ecarts} ecart(s)")
        else:
            print(f"ok     {nom}")
    echecs += controle_des_textes()
    if echecs:
        print(f"\n{echecs} cas en echec", file=sys.stderr)
        return 1
    total = len(CAS) + len(CAS_SORTS) + len(CAS_SORTS_D_OBJETS) + len(CAS_INFOS) + len(CAS_ARMES) + len(CAS_NOTES)
    print(f"\n{total} cas verifies")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
