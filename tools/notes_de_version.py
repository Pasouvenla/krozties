#!/usr/bin/env python3
"""Les notes d'une version, tirées des messages de commit depuis la précédente.

Chaque commit `type: sujet` se range par type : nouveautés, corrections,
donnée du jeu, documentation ; le reste (version, CI, tests) ne concerne pas
le joueur. Les deux commits qu'une note de patch laisse se résument en une
ligne qui la nomme par son titre.

    python3 tools/notes_de_version.py v0.4.2
"""

from __future__ import annotations

import json
import re
import subprocess
import sys

SECTIONS = {
    'feat': 'Nouveautés',
    'fix': 'Corrections',
    'perf': 'Corrections',
    'data': 'Donnée du jeu',
    'donnee': 'Donnée du jeu',
    'docs': 'Documentation',
}
PAQUETS = "Paquets pour macOS (Apple Silicon et Intel), Windows et Linux. L'installation est décrite dans le README."


def git(*args: str) -> str:
    return subprocess.run(['git', *args], capture_output=True, text=True, check=True).stdout


def precedente(etiquette: str) -> str | None:
    try:
        return git('describe', '--tags', '--abbrev=0', '--match', 'v*', f'{etiquette}^').strip() or None
    except subprocess.CalledProcessError:
        return None


def titre_de_note(identifiant: str, etiquette: str) -> str:
    try:
        return json.loads(git('show', f'{etiquette}:data/notes/{identifiant}.json'))['titre']
    except (subprocess.CalledProcessError, KeyError, ValueError):
        return f'note {identifiant}'


def notes(etiquette: str) -> str:
    debut = precedente(etiquette)
    plage = f'{debut}..{etiquette}' if debut else etiquette
    lignes: dict[str, list[str]] = {}
    for sujet in git('log', plage, '--no-merges', '--reverse', '--pretty=%s').splitlines():
        m = re.fullmatch(r'(\w+)(?:\([^)]*\))?!?:\s*(.+)', sujet.strip())
        if not m or m.group(1) not in SECTIONS:
            continue
        texte = m.group(2).rstrip('.')
        note = re.fullmatch(r'note (\d+)(?: .*)?', texte)
        if note:
            texte = f'Note de mise à jour prise en compte : {titre_de_note(note.group(1), etiquette)}'
        texte = texte[0].upper() + texte[1:]
        section = lignes.setdefault(SECTIONS[m.group(1)], [])
        if texte not in section:
            section.append(texte)
    blocs = [f'### {titre}\n' + '\n'.join(f'- {l}' for l in lignes[titre])
             for titre in dict.fromkeys(SECTIONS.values()) if titre in lignes]
    return '\n\n'.join([*blocs, PAQUETS]) + '\n'


if __name__ == '__main__':
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.stdout.write(notes(sys.argv[1]))
