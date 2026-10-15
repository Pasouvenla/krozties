#!/usr/bin/env python3
"""Les notes de version : les lignes tirées des commits, l'ajout d'une version
au fichier, et le fichier de l'application lui-même.

Lancer : ./tools/test_notes_de_version.py
"""

import copy
import datetime
import importlib.util
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("notes_de_version", ROOT / "notes_de_version.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

TITRES = {"2459343": "Patch notes 3.7.10.10 du 13/10/2026"}
titre = lambda i: TITRES.get(i, f"note {i}")

SUJETS = [
    "feat: les résistances de la cible se renseignent désormais dans la Rotation",
    "chore: version 0.4.2",
    "fix: la Gonflable Transmutée ne lance plus Brise Automnale qu'un tour sur deux.",
    "donnee: note 2459343",
    "donnee: note 2459343 appliquee par la CI",
    "ci: notes de version tirées des commits",
    "docs: installation par système",
]

ENTREE_TIREE = {
    "version": "0.6.0",
    "date": "2026-10-20",
    "rubriques": [
        {"titre": "Nouveautés", "lignes": ["Les résistances de la cible se renseignent désormais dans la Rotation."]},
        {"titre": "Corrections", "lignes": ["La Gonflable Transmutée ne lance plus Brise Automnale qu'un tour sur deux."]},
        {"titre": "Données du jeu", "lignes": ["Les données du jeu suivent le patch 3.7.10.10 du 13/10/2026."]},
        {"titre": "Documentation", "lignes": ["Installation par système."]},
    ],
}

CAS = [
    ("une phrase prend une majuscule et un point", lambda: module.phrase("exporter la liste"), "Exporter la liste."),
    ("une phrase déjà ponctuée ne prend rien", lambda: module.phrase("Déjà fini."), "Déjà fini."),
    ("un patch se nomme par son numéro et sa date",
     lambda: module.ligne_de_patch("Patch notes 3.7.10.10 du 13/10/2026"),
     "Les données du jeu suivent le patch 3.7.10.10 du 13/10/2026."),
    ("une mise à jour sans date", lambda: module.ligne_de_patch("Mise à jour 3.7"),
     "Les données du jeu suivent le patch 3.7."),
    ("un titre sans numéro reste cité", lambda: module.ligne_de_patch("Notes de la bêta"),
     "Les données du jeu suivent « Notes de la bêta »."),
    ("sans entrée « prochaine », les commits font les notes, une seule ligne par note de patch",
     lambda: module.ajouter({"versions": []}, "0.6.0", "2026-10-20", SUJETS, titre)["versions"][0],
     ENTREE_TIREE),
    ("l'entrée « prochaine » prend le numéro, et la note de patch s'y ajoute",
     lambda: module.ajouter(
         {"versions": [{"version": "prochaine", "rubriques": [
             {"titre": "Nouveautés", "lignes": ["Ajout des cibles à résistances."]}]},
             {"version": "0.5.0", "date": "2026-10-15", "rubriques": []}]},
         "0.6.0", "2026-10-20", SUJETS, titre)["versions"][:2],
     [{"version": "0.6.0", "date": "2026-10-20", "rubriques": [
         {"titre": "Nouveautés", "lignes": ["Ajout des cibles à résistances."]},
         {"titre": "Données du jeu", "lignes": ["Les données du jeu suivent le patch 3.7.10.10 du 13/10/2026."]}]},
      {"version": "0.5.0", "date": "2026-10-15", "rubriques": []}]),
    ("une version déjà notée ne change pas",
     lambda: module.ajouter({"versions": [copy.deepcopy(ENTREE_TIREE)]}, "0.6.0", "2026-10-21", SUJETS, titre),
     {"versions": [ENTREE_TIREE]}),
    ("la page GitHub : une rubrique par titre, puis les paquets",
     lambda: module.markdown({"rubriques": [{"titre": "Corrections", "lignes": ["Une ligne."]}]}),
     "### Corrections\n- Une ligne.\n\n" + module.PAQUETS + "\n"),
]


def controle_du_fichier() -> int:
    """Le fichier de l'application : une version par numéro, datée, de la plus
    récente à la plus ancienne, et des lignes de note de patch."""
    echecs = 0
    versions = module.lire()["versions"]
    numeros = [v["version"] for v in versions if v["version"] != module.PROCHAINE]
    cle = lambda n: tuple(int(x) for x in n.split("."))
    if numeros != sorted(set(numeros), key=cle, reverse=True):
        echecs += 1
        print(f"ECHEC  versions en double ou dans le désordre : {numeros}")
    for v in versions:
        if v["version"] == module.PROCHAINE:
            continue
        try:
            datetime.date.fromisoformat(v["date"])
        except (KeyError, ValueError):
            echecs += 1
            print(f"ECHEC  {v['version']} sans date lisible")
        for r in v["rubriques"]:
            if r["titre"] not in module.ORDRE:
                echecs += 1
                print(f"ECHEC  {v['version']} : rubrique inconnue {r['titre']}")
            for l in r["lignes"]:
                if not re.match(r"[«A-ZÀ-Ý0-9]", l) or l[-1] not in ".!?…" or "\u2014" in l:
                    echecs += 1
                    print(f"ECHEC  {v['version']} : ligne mal formée « {l} »")
    print(f"{'ok' if not echecs else 'ECHEC'}     fichier des notes : {len(numeros)} versions")
    return echecs


def main() -> int:
    echecs = 0
    for nom, cas, attendu in CAS:
        obtenu = cas()
        if obtenu != attendu:
            echecs += 1
            print(f"ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}")
        else:
            print(f"ok     {nom}")
    echecs += controle_du_fichier()
    if echecs:
        print(f"\n{echecs} cas en echec")
        return 1
    print(f"\n{len(CAS) + 1} cas verifies")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
