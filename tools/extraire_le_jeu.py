#!/usr/bin/env python3
"""Les fichiers de donnees d'une version du jeu, tires des serveurs d'Ankama.

doduda (dofusdude, GPL v3), compile ici depuis son code a la version figee
plus bas, telecharge sur le CDN d'Ankama les paquets de donnees et de textes de
la version demandee et les deballe en JSON, sans aucune image. Le dossier
`data` obtenu est celui que lit `tools/import_jeu.py`.

    python3 tools/extraire_le_jeu.py 3.7.14.18 /chemin/jeu-3.7.14.18
    python3 tools/import_jeu.py /chemin/jeu-3.7.14.18/data

Demande git et Go 1.26 ou plus. Rien ne s'ecrit dans le depot : doduda se
compile dans le cache de l'utilisateur, les donnees vont dans le dossier donne,
qui doit etre hors du depot.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DODUDA = "https://github.com/dofusdude/doduda.git"
ETIQUETTE = "v0.7.5"
COMMIT = "59ae8b1ffd2517b1ada61adb13f63f4458f2cb2b"


def cache() -> pathlib.Path:
    return pathlib.Path(os.environ.get("XDG_CACHE_HOME") or pathlib.Path.home() / ".cache") / "krozties"


def doduda() -> pathlib.Path:
    """Le binaire de doduda, compile depuis son code a l'etiquette figee, dont
    le commit est verifie."""
    racine = cache() / f"doduda-{ETIQUETTE}"
    binaire = racine / "doduda"
    if binaire.exists():
        return binaire
    source = racine / "source"
    if not source.exists():
        subprocess.run(
            ["git", "clone", "--quiet", "--depth", "1", "--branch", ETIQUETTE, DODUDA, str(source)], check=True
        )
    tete = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "HEAD"], check=True, capture_output=True, text=True
    ).stdout.strip()
    if tete != COMMIT:
        raise SystemExit(f"doduda {ETIQUETTE} pointe sur {tete}, pas sur {COMMIT} : rien n'est compile.")
    # Les dependances suivent go.sum ; aucune autre chaine de compilation n'est
    # telechargee.
    env = dict(os.environ, GOTOOLCHAIN="local", GOFLAGS="-mod=readonly")
    subprocess.run(["go", "build", "-trimpath", "-o", str(binaire), "."], cwd=source, env=env, check=True)
    return binaire


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("version", help="la version du jeu, 3.7.14.18 par exemple")
    parser.add_argument("dossier", type=pathlib.Path, help="ou deposer les donnees, hors du depot")
    parser.add_argument("--jobs", type=int, default=4, help="travaux en parallele")
    args = parser.parse_args()
    dossier = args.dossier.resolve()
    if dossier == ROOT or ROOT in dossier.parents:
        print(f"{dossier} est dans le depot : les donnees du jeu n'y vont pas.", file=sys.stderr)
        return 2
    sortie = dossier / "data"
    sortie.mkdir(parents=True, exist_ok=True)
    # doduda ecrit son manifeste dans le dossier courant.
    subprocess.run(
        [
            str(doduda()), "--headless", "--jobs", str(args.jobs), "--dofus-version", args.version,
            "-i", "images-*", "--output", str(sortie),
        ],
        cwd=dossier,
        check=True,
    )
    for attendu in ("spells.json", "spell_levels.json"):
        if not (sortie / attendu).exists():
            print(f"{attendu} manque dans {sortie}.", file=sys.stderr)
            return 1
    print(f"python3 tools/import_jeu.py {sortie}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
