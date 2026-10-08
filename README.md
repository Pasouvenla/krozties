<p align="center"><img src="crates/krozties/ui/logo.png" alt="Krozties" width="480"></p>

Krozties calcule, pour un personnage et son équipement, la rotation de sorts
qui fait le plus de dégâts sur plusieurs tours dans Dofus 3. Ses outils
(KrozTools) couvrent aussi les zones, les réseaux de pièges du Sram, les
bombes du Roublard, les portails de l'Eliotrope et la ligne de vue.

Données du jeu : version 3.7.

## État

Version 0.1.0, la première publiée.

## Mises à jour

L'application de bureau demande à GitHub, au plus une fois par jour, si une
nouvelle version est publiée, et l'annonce en bas à droite de la fenêtre.

## Installation

Chaque version dépose ses paquets sur la page des
[versions](https://github.com/Pasouvenla/krozties/releases/latest).

### macOS (Apple Silicon et Intel)

Avec [Homebrew](https://brew.sh) :

```bash
brew install --cask pasouvenla/krozties/krozties
```

Sinon, ouvrir `Krozties_<version>_universal.dmg` et glisser Krozties dans
Applications.

Krozties n'est pas signé par Apple : au premier lancement, macOS refuse de
l'ouvrir. Dans Réglages Système, Confidentialité et sécurité, cliquer sur
« Ouvrir quand même », une seule fois.

### Windows

Lancer `Krozties_<version>_x64-setup.exe`, ou le `.msi`. Krozties n'est pas
signé : si SmartScreen affiche « Windows a protégé votre ordinateur », cliquer
sur « Informations complémentaires », puis sur « Exécuter quand même ».

### Linux

- AppImage : `chmod +x Krozties_<version>_amd64.AppImage`, puis la lancer.
- Debian, Ubuntu : `sudo apt install ./Krozties_<version>_amd64.deb`
- Fedora : `sudo dnf install ./Krozties-<version>-1.x86_64.rpm`

## Licence

Le code de Krozties est publié sous licence AGPL-3.0 (fichier `LICENSE`).
Les données et visuels du jeu n'en font pas partie.

## Mentions

Certaines illustrations et éléments visuels sont la propriété d'Ankama Studio
et de Dofus. Cet outil est un outil communautaire non officiel, sans
affiliation avec Ankama.
