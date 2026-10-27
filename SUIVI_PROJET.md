# Où en est notre application ?

**Bilan du 27 octobre 2026 à 10 h 20 (heure de Paris).**
Le nouveau palier est codé, testé et compilé pour Mac Apple Silicon : **l’hôte peut publier
une prise terminée pendant la session, sans reconnecter l’invité**. Paquet : `f93e8af2`.
Les contrôles distants de cette version sont bloqués par un refus de GitHub, détaillé ci-dessous.

## À retenir

On construit **une seule application pour produire, créer à plusieurs, préparer son set et jouer**.
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport.

Le partage comprenait déjà les notes, le mixeur, les clips et les sons présents au départ.
**Maintenant, une nouvelle prise enregistrée dans un onglet local peut rejoindre le morceau
partagé pendant la connexion.** L’invité reçoit et vérifie le son avant de voir son nouveau clip.
Le son entier est publié sans les découpes, boucles ou effets du clip source.

Le paquet de développement est disponible. Il n’a pas remplacé automatiquement l’application
installée ou déjà ouverte sur ton Mac. Le multijoueur complet et l’espace DJ complet restent en développement.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à deux** | Un hôte et un invité sur le réseau local ; partager une copie MIDI/audio avec les sons chargés, en conservant les originaux. | Plus d’un invité, Internet et reconnexion. |
| **Notes MIDI** | Ajouter, modifier et supprimer les notes ; régler hauteur, durée et vélocité. | Davantage d’outils partagés et l’édition des instruments. |
| **Clips MIDI/audio** | Créer, déplacer, changer les bords, dupliquer, activer et supprimer les clips de l’arrangement. | Ajouter des pistes pendant la connexion et travailler dans les cases du lanceur. |
| **Nouvelles prises** | L’hôte publie un son terminé depuis un onglet Studio local vers une piste audio partagée existante. | Envoi par l’invité, enregistrement directement dans l’onglet partagé, publication automatique et rendu des effets/découpes. |
| **Écoute personnelle** | Chacun garde son lecteur, sa position, son démarrage et son arrêt. Une publication ne remplace pas les lecteurs. | Essai sur les deux ordinateurs de la maison. |
| **Espace DJ** | Bibliothèque persistante, import, analyse tempo/tonalité issue de rbxport, ajout d’un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

## Comment publier une nouvelle prise

1. L’hôte partage d’abord un projet contenant une **piste audio** ; l’invité rejoint avec son invitation.
2. L’hôte enregistre sa nouvelle prise dans un **onglet Studio local ordinaire**, puis termine l’enregistrement.
3. Il garde le panneau **Multiplayer** ouvert et sélectionne cet onglet local.
4. Dans **Publish take**, il choisit le son chargé, la piste audio du projet partagé et **Start beat**.
5. Il clique sur **Publish loaded take**. Le clip apparaît dans les copies partagées une fois accepté ; l’invité reçoit d’abord le son manquant.
6. Il enregistre explicitement la copie partagée pour conserver son arrangement modifié.

Le bouton **Share project + audio** reste celui du partage initial. Les copies des sons sont
conservées sous **Shared projects**, dans le dossier de données de l’application. Les nouvelles
prises collectées ont également leur dossier ; la sauvegarde et les sauvegardes de secours
conservent leurs chemins locaux, sans les transmettre à l’autre ordinateur.

Limites actuelles : **64 sons uniques par session, 64 Mio par son et 256 Mio au total**,
mono/stéréo. Supprimer un clip ne remet pas ce budget à zéro. Un son déjà vérifié par l’invité
n’est pas transféré une deuxième fois. Les WAV portables conservent le son décodé ; le format
original MP3/FLAC et ses tags ne sont pas archivés. FM Synth et Utility restent les appareils
pris en charge ; plugins externes, autres instruments et stems à plusieurs flux restent refusés.

## Ce qu’on vient de fabriquer, et pourquoi

- **Publication après connexion :** pour ajouter une nouvelle voix, prise ou boucle au morceau commun.
- **Choix explicite du son et de sa place :** pour préserver le projet local et ses fichiers d’origine.
- **Réutilisation des sons déjà reçus :** pour éviter de retransférer le même fichier à chaque placement.
- **Validation avant affichage :** un transfert incomplet n’ajoute pas de clip sans son chez l’invité.
- **Sauvegarde et réouverture :** les nouvelles prises restent retrouvables après la déconnexion.
- **Arrivée pendant une publication :** une prise publiée pendant le téléchargement initial est chargée avant l’ouverture du premier lecteur de l’invité.

## Ce qui a été vérifié

| Vérification | État constaté |
|---|---|
| Ensemble des tests sur ce Mac | **3 102 réussis, aucun échec ; 53 facultatifs ignorés.** Le total comprend Suhina et nos ajouts. |
| Prise enregistrée après la connexion | **Réussi avec le moteur natif et un micro simulé**, puis transfert chiffré, son identique et original conservé. |
| Lecteurs, publication et sauvegarde | **Réussi** : lectures indépendantes, réutilisation du même son, sauvegarde/réouverture et arrivée tardive. |
| Erreurs et limites | **Réussi** : ancienne révision, mauvaise piste, publication forgée par un invité, fichiers incorrects, quotas, transfert interrompu et nettoyage des copies abandonnées. |
| Interface | **Réussi** : bouton cliqué dans les tests, conservation des onglets et affichage contrôlé visuellement. |
| Paquet Mac Apple Silicon `f93e8af2` | **Compilé et vérifié après extraction** : signature locale, trois exécutables, version de l’outil, sources exactes et sommes de contrôle. |
| Nouveaux contrôles GitHub Mac/Windows/Linux | **Non lancés : GitHub refuse le déclenchement. Aucun résultat annoncé pour cette version.** |
| Deux ordinateurs physiques | **Pas encore testé**, comme convenu. |

Les tests complets portent sur le code `374c294a04847230c7226c4e7738b17a0eb5822d`.
Le paquet vient de `f93e8af28e4d8be992c1b139568908b0aa000391` : le seul changement entre
ces deux révisions est l’ajout d’une relance manuelle aux contrôles GitHub. Les sources Rust,
les dépendances et les tests sont identiques ; la compilation du paquet utilise cette dernière révision.

**Blocage GitHub constaté le 27 octobre à 10 h 19 :** le lancement manuel renvoie
`HTTP 422: Actions has been disabled for this repository.` Pourtant, la lecture du réglage
GitHub renvoie `enabled: true`. La cause de ce désaccord n’est pas établie. Aucun nouveau
contrôle n’était visible pour ces révisions ; cela ne signifie pas un échec de compilation.
Le réglage général du dépôt n’a pas été modifié. L’entrée de relance manuelle est prête pour
quand GitHub acceptera à nouveau les exécutions.

La version précédente, **`9a6b7762`**, a bien terminé ses [contrôles Mac, Windows et Linux](https://github.com/Nozomagarion/suhina-daw/actions/runs/40419500365),
constaté le 27 octobre à 10 h 01. Ce résultat ne valide pas la nouvelle publication des prises.
Les états de ce fichier sont un relevé daté ; ils ne se rafraîchissent pas tout seuls.

## Où trouver la version actuelle

- **Code et présentation :** [notre dépôt GitHub](https://github.com/Nozomagarion/suhina-daw), branche `feature/multiplayer` affichée dès l’accueil.
- **Paquet local :** dossier `builds` voisin du dépôt, `suhina-multiplayer-takes-macos-arm64-f93e8af2.zip`.
- **Dans le paquet :** `Suhina.app`, les outils nécessaires, ce suivi et les sources exactes du commit livré.
- **Somme de contrôle :** fichier `.zip.sha256` à côté du ZIP.
- **Preuves locales :** dossier `validation/2026-10-27` voisin du dépôt : journaux, captures et script de fabrication du paquet.

**Les deux participants devront utiliser le protocole 6**, donc cette même nouvelle version.
Les anciens paquets utilisant les protocoles 4 ou 5 ne peuvent pas rejoindre ces sessions.
Le canal commun de mises à jour n’est pas encore publié. Les nouveautés Suhina/rbxport doivent
être intégrées à notre version, testées puis distribuées ensemble pour conserver le multijoueur.

## Ce qui vient ensuite

**Prochain développement : ajouter des pistes pendant la session. Statut : à commencer.**
Critère : créer une piste audio sans reconnecter les participants, y publier un son et conserver
les deux projets identiques. Cela évitera de préparer toutes les pistes avant le premier partage.

La suite reste : envoi de prises par l’invité, appareils et participants supplémentaires,
reconnexion, installation/mise à jour commune, puis préparation et performance DJ avancées.
**L’essai sur deux ordinateurs à la maison reste prévu plus tard, comme tu l’as demandé.**

## Comment ce suivi sera tenu

Ce fichier reste le point de référence et sera actualisé à chaque palier ou blocage significatif.
Les réponses résumeront le changement, son intérêt, les vérifications et la prochaine étape.
Les anciens fichiers `AUDIT_…` restent historiques. Le [suivi détaillé](fork-progress.json) compte
les étapes vérifiables ; ses barres ne mesurent pas un pourcentage global du logiciel terminé.
