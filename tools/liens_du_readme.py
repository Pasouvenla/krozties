#!/usr/bin/env python3
"""Les liens de téléchargement du README, vers les paquets d'une version.

Les paquets portent le numéro de leur version (`Krozties_0.9.0_x64-setup.exe`) :
à chaque montée de version, les liens directs du README et les noms de fichier
de ses commandes passent à la nouvelle.

    python3 tools/liens_du_readme.py v0.9.0
    python3 tools/liens_du_readme.py --test
"""

from __future__ import annotations

import pathlib
import re
import sys

README = pathlib.Path(__file__).resolve().parent.parent / 'README.md'
PAQUETS = r'(x64-setup\.exe|x64\.msi|universal\.dmg|amd64\.AppImage|amd64\.deb|x86_64\.rpm)'


def reecrire(texte: str, numero: str) -> str:
    texte = re.sub(r'releases/(?:latest/download|download/v\d+(?:\.\d+)+)/', f'releases/download/v{numero}/', texte)
    return re.sub(rf'Krozties_(?:\d+(?:\.\d+)+_)?{PAQUETS}', rf'Krozties_{numero}_\1', texte)


AVANT = """[`.dmg`](https://github.com/Pasouvenla/krozties/releases/latest/download/Krozties_universal.dmg)
[versions](https://github.com/Pasouvenla/krozties/releases/latest).
- [AppImage](https://github.com/Pasouvenla/krozties/releases/latest/download/Krozties_amd64.AppImage) : `chmod +x Krozties_amd64.AppImage`,
  `sudo apt install ./Krozties_amd64.deb`
"""
APRES = """[`.dmg`](https://github.com/Pasouvenla/krozties/releases/download/v0.9.0/Krozties_0.9.0_universal.dmg)
[versions](https://github.com/Pasouvenla/krozties/releases/latest).
- [AppImage](https://github.com/Pasouvenla/krozties/releases/download/v0.9.0/Krozties_0.9.0_amd64.AppImage) : `chmod +x Krozties_0.9.0_amd64.AppImage`,
  `sudo apt install ./Krozties_0.9.0_amd64.deb`
"""


def test() -> int:
    echecs = 0
    for nom, obtenu, attendu in [
        ('les liens « latest » et les noms passent à la version', reecrire(AVANT, '0.9.0'), APRES),
        ('une version déjà écrite passe à la suivante', reecrire(APRES, '0.10.0'), APRES.replace('0.9.0', '0.10.0')),
        ('la page des versions ne change pas', reecrire(APRES, '0.9.0'), APRES),
    ]:
        if obtenu != attendu:
            echecs += 1
            print(f'ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}')
        else:
            print(f'ok     {nom}')
    return 1 if echecs else 0


if __name__ == '__main__':
    if sys.argv[1:] == ['--test']:
        raise SystemExit(test())
    if len(sys.argv) != 2 or not sys.argv[1].startswith('v'):
        sys.exit(__doc__)
    README.write_text(reecrire(README.read_text(encoding='utf-8'), sys.argv[1].lstrip('v')), encoding='utf-8')
