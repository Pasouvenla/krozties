#!/usr/bin/env python3
"""Les notes de version : le fichier de l'application et la page GitHub.

`crates/krozties/ui/notes-de-version.json` porte les notes de chaque version,
de la plus récente à la plus ancienne : l'application les affiche, la page de
chaque version sur GitHub les reprend.

    python3 tools/notes_de_version.py v0.5.0
        le texte de la page de la version ;
    python3 tools/notes_de_version.py --ajouter v0.6.0 [AAAA-MM-JJ]
        la version ajoutée au fichier, à la montée de version.

À l'ajout, une entrée « prochaine » rédigée à la main prend le numéro ; sans
elle, les notes se tirent des commits depuis l'étiquette précédente, un sujet
`type: phrase` par ligne. Les notes de patch d'Ankama que le bot applique
s'ajoutent dans les deux cas.
"""

from __future__ import annotations

import datetime
import json
import pathlib
import re
import subprocess
import sys

FICHIER = pathlib.Path(__file__).resolve().parent.parent / 'crates/krozties/ui/notes-de-version.json'
RUBRIQUES = {
    'feat': 'Nouveautés',
    'fix': 'Corrections',
    'perf': 'Corrections',
    'data': 'Données du jeu',
    'donnee': 'Données du jeu',
    'docs': 'Documentation',
}
ORDRE = list(dict.fromkeys(RUBRIQUES.values()))
PROCHAINE = 'prochaine'
PAQUETS = "Paquets pour macOS (Apple Silicon et Intel) et Windows. L'installation est décrite dans le README."


def git(*args: str) -> str:
    return subprocess.run(['git', *args], capture_output=True, text=True, check=True).stdout


def etiquette_avant(ref: str) -> str | None:
    try:
        return git('describe', '--tags', '--abbrev=0', '--match', 'v*', ref).strip() or None
    except subprocess.CalledProcessError:
        return None


def phrase(texte: str) -> str:
    """Une ligne de note de patch : une majuscule, un point."""
    texte = texte.strip()
    if not texte:
        return texte
    texte = texte[0].upper() + texte[1:]
    return texte if texte[-1] in '.!?…' else texte + '.'


def ligne_de_patch(titre: str) -> str:
    """Une note d'Ankama appliquée, d'après son titre."""
    numero = re.search(r'\d+(?:\.\d+)+', titre)
    date = re.search(r'du (\d{2}/\d{2}/\d{4})', titre)
    if not numero:
        return f'Les données du jeu suivent « {titre} ».'
    return f'Les données du jeu suivent le patch {numero.group(0)}' + (f' du {date.group(1)}' if date else '') + '.'


def titre_de_note(identifiant: str, ref: str) -> str:
    try:
        return json.loads(git('show', f'{ref}:data/notes/{identifiant}.json'))['titre']
    except (subprocess.CalledProcessError, KeyError, ValueError):
        return f'note {identifiant}'


def lignes(sujets: list[str], titre=lambda i: f'note {i}') -> dict[str, list[str]]:
    """Les sujets de commit `type: phrase`, rangés par rubrique."""
    rangees: dict[str, list[str]] = {}
    for sujet in sujets:
        m = re.fullmatch(r'(\w+)(?:\([^)]*\))?!?:\s*(.+)', sujet.strip())
        if not m or m.group(1) not in RUBRIQUES:
            continue
        note = re.fullmatch(r'note (\d+)(?: .*)?', m.group(2).strip())
        texte = ligne_de_patch(titre(note.group(1))) if note else phrase(m.group(2))
        rubrique = rangees.setdefault(RUBRIQUES[m.group(1)], [])
        if texte not in rubrique:
            rubrique.append(texte)
    return rangees


def en_rubriques(rangees: dict[str, list[str]]) -> list[dict]:
    return [{'titre': t, 'lignes': rangees[t]} for t in ORDRE if rangees.get(t)]


def sujets(debut: str | None, fin: str) -> list[str]:
    plage = f'{debut}..{fin}' if debut else fin
    return git('log', plage, '--no-merges', '--reverse', '--pretty=%s').splitlines()


def lire() -> dict:
    return json.loads(FICHIER.read_text(encoding='utf-8'))


def ecrire(donnees: dict) -> None:
    FICHIER.write_text(json.dumps(donnees, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def ajouter(donnees: dict, numero: str, date: str, depuis: list[str], titre) -> dict:
    """La version `numero` en tête des notes : l'entrée « prochaine » si elle
    existe, avec les notes d'Ankama de `depuis` en plus ; sinon les commits."""
    versions = donnees['versions']
    if any(v['version'] == numero for v in versions):
        return donnees
    tirees = lignes(depuis, titre)
    prochaine = next((v for v in versions if v['version'] == PROCHAINE), None)
    if prochaine:
        versions.remove(prochaine)
        rangees = {r['titre']: list(r['lignes']) for r in prochaine.get('rubriques', [])}
        for ligne in [l for l in tirees.get('Données du jeu', []) if l.startswith('Les données du jeu suivent')]:
            rangees.setdefault('Données du jeu', [])
            if ligne not in rangees['Données du jeu']:
                rangees['Données du jeu'].append(ligne)
        tirees = rangees
    versions.insert(0, {'version': numero, 'date': date, 'rubriques': en_rubriques(tirees)})
    return donnees


def markdown(entree: dict) -> str:
    blocs = [f"### {r['titre']}\n" + '\n'.join(f'- {l}' for l in r['lignes']) for r in entree['rubriques']]
    return '\n\n'.join([*blocs, PAQUETS]) + '\n'


def page(etiquette: str) -> str:
    numero = etiquette.lstrip('v')
    entree = next((v for v in lire()['versions'] if v['version'] == numero), None)
    if entree is None:
        depuis = sujets(etiquette_avant(f'{etiquette}^'), etiquette)
        entree = {'rubriques': en_rubriques(lignes(depuis, lambda i: titre_de_note(i, etiquette)))}
    return markdown(entree)


if __name__ == '__main__':
    arguments = sys.argv[1:]
    if len(arguments) == 1 and arguments[0].startswith('v'):
        sys.stdout.write(page(arguments[0]))
    elif len(arguments) in (2, 3) and arguments[0] == '--ajouter':
        date = arguments[2] if len(arguments) == 3 else datetime.date.today().isoformat()
        depuis = sujets(etiquette_avant('HEAD'), 'HEAD')
        ecrire(ajouter(lire(), arguments[1].lstrip('v'), date, depuis, lambda i: titre_de_note(i, 'HEAD')))
    else:
        sys.exit(__doc__)
