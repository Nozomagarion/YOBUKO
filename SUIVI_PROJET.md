# Où en est notre application ?

**Bilan du 27 octobre 2026 — envoi de prises par l’invité livré et validé localement.**
Les deux participants peuvent maintenant **publier une prise terminée pendant la connexion**.
L’hôte reçoit et vérifie le son de l’invité avant d’ajouter son clip au morceau partagé.
Paquet Mac : **`de4dae38`**, protocole 8. Les contrôles distants restent bloqués par GitHub.

## À retenir

On construit **une seule application pour produire, créer à plusieurs, préparer son set et jouer**.
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport.

Le partage comprenait déjà les notes, le mixeur, les clips et l’ajout de pistes. **L’invité peut
désormais apporter ses propres prises au morceau commun**, comme l’hôte, sans reconnecter la
session. La préparation du son se fait en arrière-plan ; l’envoi est affiché, puis le clip
apparaît une fois accepté. Les originaux restent dans leurs onglets et fichiers locaux.
La publication utilise le son entier, sans les découpes, boucles ou effets du clip source.

Le paquet de développement est disponible. Il n’a pas remplacé automatiquement l’application
installée ou déjà ouverte sur ton Mac. Le multijoueur complet et l’espace DJ complet restent en développement.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à deux** | Un hôte et un invité sur le réseau local ; partager une copie MIDI/audio avec les sons chargés, en conservant les originaux. | Plus d’un invité, Internet et reconnexion. |
| **Notes MIDI** | Ajouter, modifier et supprimer les notes ; régler hauteur, durée et vélocité. | Davantage d’outils partagés et l’édition des instruments. |
| **Pistes** | Les deux participants peuvent ajouter une piste audio ou MIDI/FM Synth pendant la session. | Suppression, réorganisation/groupes et choix partagé des appareils. |
| **Clips MIDI/audio** | Créer, déplacer, changer les bords, dupliquer, activer et supprimer les clips de l’arrangement. | Édition dans les cases du lanceur et davantage d’opérations. |
| **Nouvelles prises** | L’hôte et l’invité publient un son terminé depuis un onglet Studio local vers une piste audio partagée. L’hôte vérifie les envois. | Enregistrement directement dans l’onglet partagé, publication automatique et rendu des effets/découpes. |
| **Écoute personnelle** | Chacun garde son lecteur, sa position, son démarrage et son arrêt. Une publication ne remplace pas les lecteurs. | Essai sur les deux ordinateurs de la maison. |
| **Espace DJ** | Bibliothèque persistante, import, analyse tempo/tonalité issue de rbxport, ajout d’un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

## Comment ajouter une piste

1. Ouvrir **Multiplayer** et créer ou rejoindre une session.
2. Dans **Tracks**, saisir **New track name**.
3. Choisir **Audio track** ou **MIDI track · FM Synth**, puis cliquer sur **Add shared track**.
4. La piste apparaît dans les deux copies. Dans **Clips**, créer son clip ; utiliser **MIDI notes** pour composer sur une piste MIDI, ou **Publish take** pour remplir une piste audio.
5. Si le projet a changé depuis l’ouverture du formulaire, utiliser **Refresh track draft**, puis ressaisir le nom.

La limite est de **64 pistes**, retours et groupes compris. Les pistes créées n’activent aucun
micro, entrée MIDI ou monitoring local. Les pistes MIDI ont FM Synth par défaut ; les pistes
audio sont vides. Les clips et les réglages déjà présents sont conservés. Les préférences
visuelles locales ne modifient plus les couleurs stockées dans l’onglet partagé pendant la connexion.

## Comment publier une nouvelle prise

1. L’hôte crée ou partage un projet ; l’invité rejoint avec son invitation. Une **piste audio** peut être ajoutée ensuite dans **Tracks**.
2. Le participant qui veut publier enregistre sa prise dans un **onglet Studio local ordinaire**, puis termine l’enregistrement.
3. Il garde le panneau **Multiplayer** ouvert et sélectionne cet onglet local.
4. Dans **Publish take**, il choisit le son chargé, la piste audio du projet partagé et **Start beat**.
5. Il clique sur **Publish loaded take**. S’il est invité, l’hôte demande et vérifie les morceaux du fichier. Le clip apparaît dans les deux copies après confirmation.
6. Chacun enregistre explicitement sa copie partagée pour conserver l’arrangement modifié.

**Preparing take…** puis **Sending take** indiquent la préparation et l’envoi. Les lecteurs
restent indépendants. Si le morceau change pendant le transfert, l’hôte peut refuser la prise
périmée : attendre la synchronisation puis relancer la publication. Aucun ancien arrangement
n’est écrasé. Le transfert utilise la connexion existante ; les modifications réseau peuvent
attendre sa fin, tandis que l’écoute locale continue.

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

- **Envoi depuis l’invité :** pour que chacun puisse apporter une nouvelle voix, prise ou boucle.
- **Préparation en arrière-plan :** pour continuer à traiter la connexion pendant la copie locale du son.
- **Vérification par l’hôte :** taille, identité du fichier, format audio et révision du morceau avant son ajout.
- **Son et clip livrés ensemble à l’interface :** pour ne pas afficher un clip dont le son n’est pas encore disponible.
- **Réutilisation des sons :** un fichier déjà vérifié n’est pas renvoyé inutilement à son auteur ni recopié une seconde fois chez l’hôte.
- **Refus et nettoyage :** un transfert incomplet ou une prise périmée n’ajoute pas de clip ; les copies abandonnées sont retirées, les originaux conservés.

## Ce qui a été vérifié

| Vérification | État constaté |
|---|---|
| Ensemble des tests sur ce Mac | **3 119 réussis, aucun échec ; 54 non lancés dans ce passage standard.** Le contrôle visuel de l’invité a été exécuté séparément. Le total comprend Suhina et nos ajouts. |
| Ajout de pistes pendant la connexion | **Réussi dans les deux sens** : piste MIDI puis notes audibles ; piste audio puis publication d’un son, projets identiques. |
| Prise de l’invité enregistrée après la connexion | **Réussi avec le moteur natif et un micro simulé**, puis envoi chiffré à l’hôte, son identique et original conservé. |
| Lecteurs, publication et sauvegarde | **Réussi dans les deux sens** : lectures indépendantes, réutilisation du même son, sauvegarde/réouverture des deux copies et autres modifications partagées après un envoi. |
| Erreurs et limites | **Réussi** : refus avant réception des données, doublons acceptés une seule fois, modifications pendant l’envoi, mauvaise identité de fichier, WAV malformé, erreur de préparation, quotas et nettoyage. |
| Interface | **Réussi** : bouton cliqué dans les tests, conservation des onglets et affichage contrôlé visuellement. |
| Paquet Mac Apple Silicon `de4dae38` | **Compilé et vérifié après extraction** : signature locale, trois exécutables, version de l’outil, sources exactes et sommes de contrôle. |
| Nouveaux contrôles GitHub Mac/Windows/Linux | **Non lancés : GitHub refuse le déclenchement. Aucun résultat annoncé pour cette version.** |
| Deux ordinateurs physiques | **Pas encore testé**, comme convenu. |

Les tests et le paquet correspondent au code **`de4dae38047ddde709d704f0a90ecb2c86848939`**,
compilé après son enregistrement et envoyé sur notre branche GitHub.

**Blocage GitHub à nouveau constaté le 27 octobre à 21 h 11 :** le lancement manuel renvoie
`HTTP 422: Actions has been disabled for this repository.` La lecture du réglage renvoie
pourtant `enabled: true`. La cause de ce désaccord n’est pas établie. Aucun nouveau contrôle
n’était visible pour cette révision ; la validation distante n’a pas pu commencer.
Le réglage général du dépôt n’a pas été modifié. L’entrée de relance manuelle est prête pour
quand GitHub acceptera à nouveau les exécutions.

La version précédente, **`9a6b7762`**, a bien terminé ses [contrôles Mac, Windows et Linux](https://github.com/Nozomagarion/suhina-daw/actions/runs/40419500365),
constaté le 27 octobre à 10 h 01. Ce résultat ne valide pas les nouvelles fonctions de publication et de création de pistes.
Les états de ce fichier sont un relevé daté ; ils ne se rafraîchissent pas tout seuls.

## Où trouver la version actuelle

- **Code et présentation :** [notre dépôt GitHub](https://github.com/Nozomagarion/suhina-daw), branche `feature/multiplayer` affichée dès l’accueil.
- **Paquet local :** dossier `builds` voisin du dépôt, `suhina-multiplayer-guest-takes-macos-arm64-de4dae38.zip`.
- **Dans le paquet :** `Suhina.app`, les outils nécessaires, ce suivi et les sources exactes du commit livré.
- **Somme de contrôle :** fichier `.zip.sha256` à côté du ZIP.
- **Preuves locales :** dossier `validation/2026-10-27` voisin du dépôt : journaux, captures et script de fabrication du paquet.

**Les deux participants devront utiliser le protocole 8**, donc cette même nouvelle version.
Les anciens paquets utilisant les protocoles 4, 5, 6 ou 7 ne peuvent pas rejoindre ces sessions.
Le canal commun de mises à jour n’est pas encore publié. Les nouveautés Suhina/rbxport doivent
être intégrées à notre version, testées puis distribuées ensemble pour conserver le multijoueur.

## Ce qui vient ensuite

**Prochain développement : réinviter après une coupure sans redémarrer l’hôte. Statut : à commencer.**
Critère : conserver le morceau courant chez l’hôte, produire une nouvelle invitation privée,
puis rejoindre son état à jour sans écraser la dernière copie ni les modifications locales de l’invité.
Aujourd’hui, l’invitation reste à usage unique et il faut recréer un partage après la coupure.

La suite reste : reconnexion plus complète, appareils et participants supplémentaires,
installation/mise à jour commune, puis préparation et performance DJ avancées.
**L’essai sur deux ordinateurs à la maison reste prévu plus tard, comme tu l’as demandé.**

## Comment ce suivi sera tenu

Ce fichier reste le point de référence et sera actualisé à chaque palier ou blocage significatif.
Les réponses résumeront le changement, son intérêt, les vérifications et la prochaine étape.
Les anciens fichiers `AUDIT_…` restent historiques. Le [suivi détaillé](fork-progress.json) compte
les étapes vérifiables ; ses barres ne mesurent pas un pourcentage global du logiciel terminé.
