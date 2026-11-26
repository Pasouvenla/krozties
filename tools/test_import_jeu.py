#!/usr/bin/env python3
"""Lecture des niveaux de sorts du jeu : zones, regles de lancement, lignes de
degats, autres effets, pieges et glyphes poses.

Lancer : ./tools/test_import_jeu.py, ou par pytest.
"""

import importlib.util
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("import_jeu", ROOT / "import_jeu.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def effet(action, a, b, forme=80, p1=1, p2=0, cible="A", ordre=0):
    return {"actionId": action, "diceNum": a, "diceSide": b, "value": 0, "duration": 0, "delay": 0,
            "targetMask": cible, "order": ordre,
            "zoneDescr": {"shape": forme, "param1": p1, "param2": p2, "damageDecreaseStepPercent": 10,
                          "maxDamageDecreaseApplyCount": 4}}


class FauxJeu:
    def __init__(self, niveaux):
        self.par_grade = {(n["spellId"], n["grade"]): n for n in niveaux}


def test_lecture_des_niveaux() -> None:
    assert m.zone_cells("C", 2) == 13
    assert (m.zone_cells("Q", 1, 1), m.zone_cells("Q", 3, 1), m.zone_cells("Q", 1, 0)) == (4, 12, 5)
    assert m.zone_cells("l", 1, 63) is None
    assert m.zone(effet(99, 1, 2, forme=ord("l"), p2=63), portee=6)["cells"] == 6, "la ligne depuis le lanceur"
    assert m.lignes([effet(99, 6, 0)], 1)[0]["range"] == [6, 6], "un diceSide nul : valeur fixe"

    sous_sort = {"spellId": 900, "grade": 2, "effects": {"Array": [effet(97, 10, 12), effet(5, 2, 0)]}}
    brut = {
        "grade": 1, "apCost": 3, "maxCastPerTurn": 2, "maxCastPerTarget": 1, "minCastInterval": 0,
        "globalCooldown": 0, "initialCooldown": 0, "maxStack": -1, "criticalHitProbability": 10,
        "minRange": 1, "range": 5, "statesCriterion": "", "m_flags": 1 | 4 | 64,
        "effects": {"Array": [effet(98, 15, 17), effet(5, 2, 0, cible="a,A", ordre=1),
                              effet(400, 900, 2, forme=ord("C"), p1=0, ordre=2)]},
        "criticalEffect": {"Array": [effet(98, 18, 20, ordre=3)]},
    }
    n = m.niveau(FauxJeu([sous_sort]), brut, detaille=True)
    assert n["cast"] == {"in_line": True, "in_diagonal": False, "needs_line_of_sight": True,
                         "range_boostable": True, "needs_free_cell": False, "needs_taken_cell": False,
                         "needs_cell_without_portal": False, "needs_free_trap_cell": False}, n["cast"]
    assert n["range"] == [1, 5]
    assert n["normal_lines"] == [{"element": "air", "kind": "damage", "range": [15, 17], "order": 0,
                                  "zone": {"shape": "P", "size": 1, "cells": 1, "falloff_percent": 10,
                                           "falloff_steps": 4, "size2": 0}, "target": "A"}], n["normal_lines"]
    assert [x["range"] for x in n["critical_lines"]] == [[18, 20]]
    assert n["effect_ids"] == [98], "les effets du coup critique quand il en a"
    assert [e["id"] for e in n["other_effects"]] == [5, 400]
    assert [(x["range"], x["subspell"], x["placed_by"]) for x in n["placed_lines"]] == [([10, 12], 900, "piege")]
    assert n["placed_effect_ids"] == [5, 97]
    assert n["placed_trigger_zone"]["shape"] == "C" and n["placed_trigger_zone"]["cells"] == 1
    assert [e["id"] for e in n["placed_effects"]] == [97, 5]


if __name__ == "__main__":
    test_lecture_des_niveaux()
    print("import_jeu : tout passe")
