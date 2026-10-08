<p align="center"><img src="crates/krozties/ui/logo.png" alt="Krozties" width="480"></p>

Krozties calcule, pour un personnage et son équipement, la rotation de sorts
qui fait le plus de dégâts sur plusieurs tours dans Dofus 3. Ses outils
(KrozTools) couvrent aussi les zones, les réseaux de pièges du Sram, les
bombes du Roublard, les portails de l'Eliotrope et la ligne de vue.

Données du jeu : version 3.7.

## État

Version 0.1.0, la première publiée.

## Comment Krozties est fait

Krozties est développé à deux : Pasouvenla, joueur de Dofus, et Claude,
l'assistant d'IA d'Anthropic, qui a écrit l'essentiel du code sous sa
direction. Chaque règle de calcul (sorts, effets, durées, cumuls, objets) a été
confrontée à la donnée du jeu, et une partie a été vérifiée en jeu. Plus de
700 tests automatisés gardent ces règles d'une version à l'autre.

## Erreurs et signalements

Malgré ces vérifications, il reste sans doute des erreurs : une rotation qui
n'est pas la meilleure, des dégâts mal calculés, un effet mal compris. Si vous
en voyez une, ou tout autre bug, ouvrez une
[issue](https://github.com/Pasouvenla/krozties/issues) en donnant la classe et
le niveau, le lien DofusBook de l'équipement, le sort ou la rotation en cause,
ce que Krozties affiche et ce que le jeu donne. Une capture du jeu aide
beaucoup.

## Vie privée

Krozties n'a ni télémétrie, ni mesure d'audience, ni compte : rien ne part sur
votre utilisation. L'application ne contacte le réseau que dans trois cas :

- **Nouvelle version** : au plus une fois par jour, l'application de bureau
  demande à l'API de GitHub (`api.github.com`) le numéro de la dernière version
  publiée. GitHub voit alors votre adresse IP, comme pour toute page web. Si une
  version plus récente existe, une mini fenêtre l'annonce en bas à droite.
- **Import d'un équipement** : quand vous donnez un lien DofusBook,
  l'application ouvre la page de cet équipement dans une vue invisible pour le
  lire, comme le ferait votre navigateur, avec ce que la page de DofusBook
  charge elle-même.
- **Téléchargement** : le bouton « Télécharger » de la mini fenêtre ouvre la
  page des versions dans votre navigateur.

Sur votre machine, Krozties ne garde que la date de sa dernière vérification de
version et le numéro qu'il a vu. L'équipement importé n'est pas conservé.

## DofusBook

Krozties ne remplace pas DofusBook, il s'appuie dessus. On compose son
équipement sur DofusBook, et Krozties lit celui que vous lui désignez : il
ouvre sa page chez DofusBook, à votre demande, comme votre navigateur
l'afficherait. Rien d'autre n'est lu, rien n'est conservé ni renvoyé ailleurs.

Les chiffres de sorts de DofusBook nous ont servi de référence pour vérifier
nos calculs, et les caractéristiques des invocations viennent d'un relevé de
leur site. Merci à l'équipe de DofusBook pour cet outil extraordinaire.

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
