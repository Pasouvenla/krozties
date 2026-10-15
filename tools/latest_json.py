#!/usr/bin/env python3
"""Le fichier `latest.json` que lit la mise à jour de l'application.

Écrit à la publication, une fois les trois paquets déposés et signés : pour
chaque système et chaque type de paquet, l'adresse du paquet et sa signature.
Le module de mise à jour cherche `{système}-{architecture}-{paquet}`, puis
`{système}-{architecture}`.

    python3 tools/latest_json.py <version.json> <dossier des .sig> <dépôt> > latest.json
    python3 tools/latest_json.py --test
"""

from __future__ import annotations

import datetime
import json
import pathlib
import sys

# Le suffixe du paquet, et les entrées qu'il sert ; la première de chaque
# système sert aussi d'entrée par défaut.
PAQUETS = [
    ('_universal.app.tar.gz', ['darwin-aarch64-app', 'darwin-x86_64-app', 'darwin-aarch64', 'darwin-x86_64']),
    ('_x64-setup.exe', ['windows-x86_64-nsis', 'windows-x86_64']),
    ('_x64.msi', ['windows-x86_64-msi']),
    ('_amd64.AppImage', ['linux-x86_64-appimage', 'linux-x86_64']),
    ('_amd64.deb', ['linux-x86_64-deb']),
    ('_x86_64.rpm', ['linux-x86_64-rpm']),
]


def manifeste(version: dict, signatures: dict[str, str], depot: str, notes: str = '') -> dict:
    """`version` : la réponse de l'API pour la version ; `signatures` : le
    contenu de chaque `.sig`, par nom de paquet."""
    etiquette = version['tag_name']
    noms = [a['name'] for a in version['assets']]
    plateformes = {}
    for suffixe, cles in PAQUETS:
        nom = next((n for n in noms if n.endswith(suffixe)), None)
        if nom is None or nom not in signatures:
            raise SystemExit(f'paquet ou signature manquant pour {suffixe}')
        for cle in cles:
            plateformes[cle] = {
                'signature': signatures[nom].strip(),
                'url': f'https://github.com/{depot}/releases/download/{etiquette}/{nom}',
            }
    return {
        'version': etiquette.removeprefix('v'),
        'notes': notes,
        'pub_date': datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
        'platforms': plateformes,
    }


def test() -> int:
    noms = ['Krozties_0.10.0_universal.app.tar.gz', 'Krozties_0.10.0_universal.dmg', 'Krozties_0.10.0_x64-setup.exe',
            'Krozties_0.10.0_x64.msi', 'Krozties_0.10.0_amd64.AppImage', 'Krozties_0.10.0_amd64.deb',
            'Krozties_0.10.0_x86_64.rpm']
    version = {'tag_name': 'v0.10.0', 'assets': [{'name': n} for n in noms + [n + '.sig' for n in noms]]}
    signatures = {n: f'sig de {n}\n' for n in noms}
    m = manifeste(version, signatures, 'Pasouvenla/krozties')
    echecs = 0
    cas = [
        ('la version sans le v', m['version'], '0.10.0'),
        ('onze entrées', sorted(m['platforms']), sorted(c for _, cles in PAQUETS for c in cles)),
        ('le Mac prend le .app.tar.gz', m['platforms']['darwin-aarch64-app']['url'],
         'https://github.com/Pasouvenla/krozties/releases/download/v0.10.0/Krozties_0.10.0_universal.app.tar.gz'),
        ('Windows par défaut : l’installateur NSIS', m['platforms']['windows-x86_64']['url'].rsplit('/', 1)[1],
         'Krozties_0.10.0_x64-setup.exe'),
        ('la signature, sans saut de ligne', m['platforms']['linux-x86_64-deb']['signature'],
         'sig de Krozties_0.10.0_amd64.deb'),
    ]
    for nom, obtenu, attendu in cas:
        if obtenu != attendu:
            echecs += 1
            print(f'ECHEC  {nom}\n       obtenu   {obtenu}\n       attendu  {attendu}')
        else:
            print(f'ok     {nom}')
    try:
        manifeste({'tag_name': 'v0.10.0', 'assets': [{'name': noms[0]}]}, {}, 'Pasouvenla/krozties')
        echecs += 1
        print('ECHEC  une signature manquante doit arrêter la publication')
    except SystemExit:
        print('ok     une signature manquante arrête la publication')
    return 1 if echecs else 0


if __name__ == '__main__':
    if sys.argv[1:] == ['--test']:
        raise SystemExit(test())
    if len(sys.argv) not in (4, 5):
        sys.exit(__doc__)
    version = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding='utf-8'))
    dossier = pathlib.Path(sys.argv[2])
    signatures = {p.name.removesuffix('.sig'): p.read_text(encoding='utf-8') for p in dossier.glob('*.sig')}
    notes = pathlib.Path(sys.argv[4]).read_text(encoding='utf-8') if len(sys.argv) == 5 else ''
    json.dump(manifeste(version, signatures, sys.argv[3], notes), sys.stdout, ensure_ascii=False, indent=2)
    sys.stdout.write('\n')
