<p align="center">
  <img src="assets/fork/music-workflow.svg" alt="Suhina — Studio + Multijoueur + DJ. Ta musique, de A à Z." width="100%">
</p>

<h1 align="center">Studio + Multijoueur + DJ</h1>
<p align="center"><strong>Une seule application, du premier son au dernier morceau de ton set.</strong></p>

<p align="center">
  <a href="#la-vision"><img src="https://img.shields.io/badge/PRODUIRE-ffcf75?style=for-the-badge" alt="Produire"></a>
  <a href="#créer-à-plusieurs"><img src="https://img.shields.io/badge/CR%C3%89ER_ENSEMBLE-63e5c1?style=for-the-badge" alt="Créer ensemble"></a>
  <a href="#préparer-et-jouer-son-set"><img src="https://img.shields.io/badge/PR%C3%89PARER_%26_JOUER-aba7ff?style=for-the-badge" alt="Préparer et jouer"></a>
</p>

<p align="center"><a href="#la-vision">La vision</a> · <a href="#où-en-est-le-projet">Avancement</a> · <a href="#essayer-la-version-actuelle">Essayer</a> · <a href="#les-projets-qui-rendent-cela-possible">Suhina &amp; rbxport</a> · <a href="README.en.md">English / technical overview</a></p>

**Pour suivre le travail en cours : [état du projet, limites et prochaine étape](SUIVI_PROJET.md).**

## La vision

**Je construis le logiciel dans lequel j'aimerais faire toute ma musique.**

L'idée : réunir **la production musicale dans l'esprit d'Ableton Live**, **la création à
plusieurs** et **la préparation DJ apportée par rbxport**, dans une seule et même application.
Le socle de production est le logiciel libre **Suhina**. Le multijoueur et la liaison entre
studio et DJ sont les ajouts de ce projet.

Composer une idée, enregistrer une prise, construire un morceau avec ses amis, le mixer,
préparer son set et le jouer en live : garder le même fil du début à la fin.

> **Un studio pour créer. Un espace commun pour avancer ensemble. Un espace DJ pour emmener
> sa musique sur scène.**

Le projet est en développement. Cette page présente sa destination, puis distingue ce qui
fonctionne aujourd'hui de ce qui reste à construire. La partie DJ n'intègre actuellement que
le module d'analyse de rbxport ; ses autres fonctions viendront progressivement.

## De l'idée au live

| Imaginer | Produire | Créer ensemble | Préparer | Jouer |
|---|---|---|---|---|
| Poser une mélodie, un rythme, une ambiance. | Enregistrer, arranger, travailler le son et mixer son morceau. | Construire un projet commun tout en gardant sa propre écoute. | Retrouver ses rendus, analyser les morceaux et organiser son set. | Passer de la création au live et au mix DJ. |

**C'est le parcours que je veux réunir dans une seule app.** Les instruments, effets,
plugins, clips et outils de production reposent sur le travail de Suhina. Ce fork construit
les ponts vers la collaboration et le DJ, avec une interface et une distribution communes.

### Produire sa musique

Un vrai espace de création : pistes audio et MIDI, instruments, effets, clips, arrangement,
enregistrement et rendu. Le moteur et les outils de Suhina donnent les fondations du studio.
L'objectif est de pouvoir développer un morceau complet et l'emmener jusqu'à sa préparation
pour le live, en conservant le lien avec son projet d'origine.

### Créer à plusieurs

Avancer sur la même musique avec ses amis. Pouvoir travailler sur un projet partagé tout en
écoutant une autre partie du morceau que les autres : chacun garde sa lecture et sa position.

Le prototype relie un hôte et jusqu’à trois invités sur le réseau local. Il peut maintenant transmettre
une copie d’un projet MIDI/audio ouvert, avec ses notes, ses clips et ses réglages FM Synth/Utility,
ou créer une démonstration. Les notes MIDI, noms, volumes et panoramiques se modifient ensemble.
Un éditeur permet d’ajouter, modifier et supprimer une note ; les modifications concurrentes
sont vérifiées par l’hôte. L’onglet **Clips** permet aussi de créer, déplacer, redimensionner,
dupliquer, activer et supprimer des clips MIDI/audio. Les sons déjà chargés peuvent être copiés vers l’invité sous forme de WAV portables.
Chaque participant peut aussi publier une nouvelle prise terminée depuis un onglet local : le son arrive chez l’invité puis son clip apparaît, sans recréer la session.
L’onglet **Tracks** permet à chacun d’ajouter une piste audio vide ou une piste MIDI avec FM Synth pendant la connexion.
L’onglet **Devices** permet d’ajouter et retirer FM Synth/Utility pendant le partage, puis de modifier leurs réglages.
Choisir **Device track**, puis **Add FM Synth** ou **Add Utility** ; **Remove selected device** retire l’appareil sélectionné.
Après un ajout ou retrait, il faut choisir à nouveau l’appareil pour éviter de réutiliser une ancienne sélection.
Les contrôles reprennent les plages et types du studio ; on prépare une valeur, puis on l’applique
aux deux copies. Un brouillon périmé doit être rafraîchi avant l’envoi.
Après le départ de l’invité, **New invitation** permet de le réinviter sans fermer le morceau de l’hôte.
Il retrouve l’état actuel dans un nouvel onglet ; son ancienne copie et ses modifications locales sont conservées.

### Préparer et jouer son set

Passer du morceau que l'on produit au morceau que l'on joue. Une bibliothèque DJ dans la même
application, l'analyse du tempo et de la tonalité et des playlists persistantes pour préparer
l’ordre du set. La préécoute et les points cue nommés permettent maintenant de préparer chaque
morceau. Une grille à tempo constant se règle par morceau, et deux platines A/B permettent
une première lecture de set dans la même application.

rbxport apporte une base libre pour cette direction. Aujourd'hui, son analyse est intégrée à
la bibliothèque et un rendu du studio peut y être ajouté. Dans **DJ**, créer un set, ajouter les
morceaux depuis **Add from library**, puis utiliser les flèches pour choisir leur ordre. Les listes
et leurs noms sont conservés au redémarrage ; les fichiers audio restent à leur emplacement.
**Refresh DJ library** recharge les changements d’une autre instance et les écritures périmées
sont refusées. Avec **Prepare**, choisir une position puis **Play from position**, ou écouter un
repère avec **Play cue**. Les points cue peuvent être nommés, déplacés et supprimés ; ils restent
liés au morceau dans toutes ses playlists. **Stop audition** et le retour au Studio annulent la
lecture, y compris pendant son chargement. La forme d’onde du morceau affiche les repères et permet de choisir visuellement
le départ de la préécoute sans lancer le son. Régler **Grid BPM** et **First beat seconds**,
prévisualiser puis **Save beat grid** conserve une grille à quatre battements par mesure, distincte
du BPM analysé. Le catalogue passe en format 4 avec sauvegarde de l’ancien fichier ; les anciennes
versions du fork ne lisent pas ce catalogue migré. Deux platines A/B lisent maintenant les morceaux à leur vitesse originale, avec pause,
repositionnement, volumes et crossfader ; les cues casque demandent une paire de sorties séparée.
Le calage automatique, le tempo variable et l’export vers le matériel restent à construire.

## Où en est le projet

<img src="assets/fork/progress-fr.svg" alt="Étapes réalisées pour le multijoueur, l'espace DJ et les mises à jour communes" width="960">

<!-- fork-progress:begin -->
| Partie du projet | Étapes réalisées | Prochain objectif |
|---|---:|---|
| Créer ensemble | 16 / 18 | Sessions Internet et reconnexion automatique |
| Préparer et jouer | 9 / 10 | Export USB et compatibilité matérielle |
| Évoluer ensemble | 4 / 7 | Version commune validée sur toutes les plateformes |
<!-- fork-progress:end -->

Les barres comptent les étapes listées dans [fork-progress.json](fork-progress.json), avec des
liens vers le code ou les vérifications. Elles ne représentent pas un pourcentage de réalisation
du logiciel final, ni une estimation du travail restant. Les fonctions héritées de Suhina ont
leur [propre suivi](UPSTREAM_README.md) : elles ne sont pas réattribuées à ce fork.

| Partie | Disponible dans le prototype | Prochaine étape |
|---|---|---|
| **Studio** | Le socle Suhina et son espace de production ; rendu d'un projet vers la bibliothèque DJ. | Approfondir les liens entre projet, collaboration et préparation du set. |
| **Multijoueur** | Session LAN hôte + trois invités maximum, copie MIDI/audio du projet ouvert ou démo ; jusqu’à 64 pistes, édition des notes et du placement des clips MIDI/audio, grille du lanceur et scènes partagées, publication de prises terminées par les deux participants, réglages FM Synth/Utility et noms/volumes/panoramiques partagés et lectures indépendantes. | Internet et reconnexion automatique. |
| **DJ** | Bibliothèque, playlists, cues et grilles à tempo constant persistants ; analyse rbxport, préécoute et forme d’onde. | Synchronisation, tempo et compatibilité matérielle. |
| **Mises à jour** | Comparaison des sources GitHub dans les paramètres, outils d'intégration et vérification d'un paquet commun. | Valider et publier le canal commun, puis activer l'intégration périodique. |

Le trafic chiffré est testé entre deux services sur le même ordinateur, avec des vérifications
sur deux projets de bureau et des moteurs audio indépendants. **Le test sur deux ordinateurs
physiques reste à faire.** Le document partagé est limité à 2 Mio et aux appareils FM Synth/Utility
sans état opaque. Les notes se modifient dans **MIDI notes**, et l’arrangement dans **Clips**. Les déplacements
conservent les clips voisins ; les brouillons obsolètes doivent être rafraîchis. Les sons présents au départ peuvent être transmis par **Share project + audio**. L’onglet **Publish take** permet à chacun de publier le son entier d’une prise terminée depuis un autre onglet Studio vers une piste audio existante, au temps choisi. L’onglet **Tracks** permet d’ajouter les pistes nécessaires sans repartir d’un nouveau partage. Toutes les applications doivent utiliser le protocole 13. L’invité envoie sa prise à l’hôte pour vérification avant son ajout au morceau. L’enregistrement direct dans l’onglet partagé et les plugins restent à construire.

Pour inviter plusieurs musiciens, créer une nouvelle invitation pour chacun avec **New invitation**,
après l’arrivée du précédent. Les invités déjà connectés restent présents. Le compteur **Guests: N / 3**
affiche les places occupées ; un départ libère une place sans fermer la session des autres.
[Invitations et limites](docs/spec/lan-multiple-guests.md).

Dans **Multiplayer → Launcher**, vous pouvez copier vos clips vers la grille, créer des cases,
organiser et dupliquer les scènes, puis les lancer chacun de votre côté. Les copies conservent
les originaux ; les scènes et cases modifiées sont confirmées par l’hôte. Les sélections périmées
sont annulées avant une nouvelle modification. [Fonctionnement](docs/spec/lan-launcher-editing.md).

La mise à jour du socle Suhina du 29 octobre intègre le `main` officiel `7753ffa3`, soit
56 commits supplémentaires : paramètres réorganisés, corrections d’automation, rendu et cache
des styles, blocs DSP partagés. Les égaliseurs et courbes d’automation du palier précédent restent présents. Les anciens projets
restent lisibles ; les nouveaux enregistrements de projet utilisent le format v4. Le partage
vérifie aussi la révision du moteur. [Compatibilité et limites](docs/spec/lan-upstream-20261028.md).

## Essayer la version actuelle

Pour un premier essai à plusieurs : [guide multijoueur en français](GUIDE_MULTIPLAYER.md).

Le dépôt s’ouvre directement sur **`feature/multiplayer`**, la branche de cette application.
La branche `main` reste le miroir intact du projet Suhina d’origine. [Ouvrir la branche du projet](https://github.com/Nozomagarion/suhina-daw/tree/feature/multiplayer#readme)
pour voir cette présentation et les ajouts.

Dans l'application, trois accès : **Studio**, **Multiplayer** et **DJ**.

1. Ouvrir son projet dans **Studio**, puis **Multiplayer → Share project + audio**
   pour inclure les sons déjà chargés, ou **Share current MIDI project** pour une copie MIDI seule,
   sur l’ordinateur hôte. **Create LAN session** reste disponible pour la démonstration.
2. Copier l'invitation privée avec **Copy invitation**.
3. Sur l'autre instance, coller l'invitation puis choisir **Join LAN session**.
4. Dans **MIDI notes**, choisir un clip et ajouter/modifier/supprimer une note.
   **Clips** permet de créer et arranger les clips MIDI ; **Mixer** partage les noms, volumes
   et panoramiques. Chacun utilise **Play locally**
   et **Stop locally** pour son écoute. **Leave session** conserve le projet en local.

La copie s’ouvre dans un nouvel onglet et conserve l’original. Les plugins et autres
instruments non pris en charge sont refusés avec une explication ; les métadonnées locales et affectations matérielles
sont omises. Le mode démonstration reste utilisable sans projet personnel. Les deux participants
doivent installer la version de protocole 5 ; les anciennes invitations ne sont pas compatibles.
Les [instructions détaillées](docs/multiplayer-fork.md#try-the-desktop-lan-prototype) expliquent
les adresses réseau et les limites de ce prototype.

Le transfert audio accepte jusqu’à **64 sons, 64 Mio par son et 256 Mio au total**. Les copies
restent dans **Shared projects**, sous le dossier de données de l’application, pour pouvoir
rouvrir le morceau. Les fichiers d’origine restent intacts. Un transfert incomplet ouvre
une erreur et aucun onglet partiel.

Les paquets de développement proviennent des [contrôles GitHub du fork](https://github.com/Nozomagarion/suhina-daw/actions/workflows/multiplayer-ci.yml?query=branch%3Afeature%2Fmultiplayer).
Vérifier le commit et le résultat de sa plateforme. Le canal public commun n'est pas encore
publié ; ce projet ne propose pas encore une distribution stable.

## La même app pour tout le groupe

Le but est que chacun installe la même version et conserve le studio, le multijoueur et le DJ
lors des mises à jour de Suhina ou de rbxport.

```text
Nouveautés Suhina ───┐
                    ├── Intégration avec nos ajouts ── Tests ── Nouvelle version commune
Nouveautés rbxport ──┘                                              │
                                                          Mise à jour dans l'app
```

Les sources sont suivies par version. Les nouveautés passent d'abord dans une intégration
isolée avec nos modules, puis dans les vérifications avant publication. Les conflits sont
résolus à cette étape. Les amis installent ensuite le paquet combiné validé.

Le bouton **Settings → Agents and updates → GitHub updates…** compare les sources et cherche une version
commune disponible. La récupération périodique et la publication commune restent à activer.
[Comment les mises à jour sont préparées](docs/fork-distribution.md).

## Les projets qui rendent cela possible

**Ce logiciel se construit grâce au travail de deux projets libres et de leurs auteurs.**

| Fondation | Apport au projet |
|---|---|
| **[Suhina](https://github.com/suhina-daw/suhina-daw)** et ses contributeurs | Le logiciel de production : interface native, moteur audio, projets, instruments, effets, hébergement des plugins et outils de création. C'est la fondation du studio. |
| **[rbxport](https://github.com/chrisle/rbxport)**, par chrisle et ses contributeurs | Le module `rbl-analysis` utilisé ici pour l'analyse du tempo et de la tonalité. Il fournit une première brique vers la préparation DJ ; le reste de rbxport n'est pas encore intégré. |
| **Ce fork** | La collaboration, l'espace DJ relié au studio, la présentation d'un parcours musical commun et les outils pour distribuer l'ensemble sans perdre ces ajouts. |

Les auteurs d'origine conservent leurs crédits et leurs droits. Voir le
[README original de Suhina](UPSTREAM_README.md), [ATTRIBUTION.md](ATTRIBUTION.md), [NOTICE](NOTICE),
la [provenance du code rbxport](third_party/rbxport-analysis/VENDORED.md) et les
[versions intégrées](fork-sources.json).

La référence à Ableton Live décrit l'idée de workflow recherchée. Le code du studio provient de
Suhina ; aucun code d'Ableton n'est intégré. Ce projet est indépendant et n'est affilié ni à
Ableton AG, ni aux équipes officielles de Suhina ou de rbxport. Les marques appartiennent à
leurs propriétaires.

## Construire et contribuer

```sh
git clone --branch feature/multiplayer https://github.com/Nozomagarion/suhina-daw.git
cd suhina-daw
cargo test --locked -p suhina-collaboration -p suhina-multiplayer -p suhina-dj -p suhina-update
cargo build --locked -p suhina -p suhina-cli -p suhina-plugin-host
```

[Prérequis de compilation](UPSTREAM_README.md#building) · [Guide du fork](docs/multiplayer-fork.md) ·
[Contrat du prototype LAN](docs/spec/lan-starter-session.md) · [Limites de sécurité](docs/security/session-sharing.md)

Les prochaines contributions utiles concernent les essais sur deux ordinateurs, les projets et
fichiers partagés, les enregistrements et la préparation DJ. Pour un retour de bug,
préciser le système, le commit, les étapes et le résultat attendu/observé. Garder les invitations
privées. Suivre [CONTRIBUTING.md](CONTRIBUTING.md) et discuter les ajouts de ce fork ici.

**Logiciel libre — GPL-3.0-or-later.** Conserver les crédits, notices et sources correspondantes
dans les distributions. Voir [LICENSE](LICENSE) et le [contrat de distribution](docs/fork-distribution.md).
