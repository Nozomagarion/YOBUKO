# Où en est notre application ?

**Développement en cours le 27 octobre 2026 (heure de Paris).**
Le nouveau code publie une prise terminée de l’hôte sans recréer la connexion. Les tests ciblés
réseau et interface et la suite complète passent : **3 102 réussis, aucun échec, 53 ignorés**.
La compilation et le paquet de ce nouveau palier restent à livrer.
Le paquet précédent `9a6b7762` ne possède pas encore cette fonction.

## À retenir

On construit **une seule application pour produire, créer à plusieurs, préparer son set et jouer**.
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport.

**Nouveau travail : publier les prises terminées pendant la session.**
L’hôte choisit un son chargé dans un onglet Studio local, une piste audio partagée existante
et son point de départ. L’invité reçoit le son puis son nouveau clip, sans reconnexion.
Le son entier est publié sans les découpes, boucles ou effets du clip source.

Les nouvelles fonctions sont dans un **paquet de développement compilé**. Les dernières étapes
n’ont pas remplacé automatiquement l’application déjà installée ou ouverte sur ton Mac.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à deux** | Un hôte crée une session, un invité la rejoint sur le réseau local. Une copie MIDI/audio peut être partagée avec les sons déjà chargés, en conservant l’original. | Envoi de prises par l’invité, plugins et participants supplémentaires. |
| **Notes MIDI** | Ajouter, modifier et supprimer les notes ; régler leur hauteur, durée et vélocité. | Étendre les outils partagés et l’édition des instruments. |
| **Clips MIDI/audio** | Créer, déplacer, changer les bords, dupliquer, activer et supprimer les clips de l’arrangement. | Partager davantage d’opérations du studio, dont l’ajout de pistes et les déplacements entre cases du lanceur. |
| **Écoute personnelle** | Chacun peut écouter une autre partie, démarrer et arrêter sa propre lecture. | Vérifier ce comportement sur les deux ordinateurs de la maison. |
| **Espace DJ** | Importer des morceaux dans une bibliothèque conservée sur le disque, analyser tempo et tonalité avec du code de rbxport, y ajouter un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Un bouton dans les paramètres compare les versions GitHub. Les outils suivent les deux projets d’origine et vérifient les paquets de notre application. | Activer l’intégration régulière des nouveautés, leur publication et la réception automatique d’une version commune par tous. |

Le travail à deux passe actuellement par le panneau **Multiplayer**, avec les onglets
**Mixer**, **MIDI notes** et **Clips**. L’édition partagée couvre ces commandes précises.

**Ce que fait le nouveau bouton « Share project + audio » :** il prend les sons déjà chargés
dans le projet et les copie en WAV portables, sans perte supplémentaire du son décodé.
L’invité les reçoit dans un nouveau dossier puis ouvre la session avec ces sons. Un fichier
manquant ou un transfert incomplet produit une erreur, sans ouvrir de projet partiel.
Les copies complètes restent dans **Shared projects**, sous le dossier de données de l’application.

Limites : **64 sons, 64 Mio par son et 256 Mio au total**, mono/stéréo. FM Synth et Utility
restent les appareils pris en charge ; les autres instruments, les plugins externes et les
stems à plusieurs flux restent refusés. Les voix et prises déjà présentes peuvent voyager
dans la copie initiale ; le nouveau code permet leur publication explicite par l’hôte après leur enregistrement dans un onglet local. Le paquet précédent ne possède pas cette fonction.

## Ce qu’on vient de fabriquer, et pourquoi

1. **Partager ton propre projet MIDI.** Pour travailler sur ta musique, au-delà de la démonstration.
2. **Modifier les notes à deux.** Pour pouvoir composer ensemble.
3. **Modifier les clips à deux.** Pour pouvoir arranger le morceau ensemble, pas seulement changer un volume.
4. **Protéger les modifications.** Si quelqu’un a déjà changé le passage, un vieux brouillon est refusé.
   Déplacer ou agrandir un clip conserve ses voisins. Éditer une duplication protège son original.
5. **Recevoir les sons du projet.** Pour entendre le morceau chez l’invité, même sans les fichiers d’origine sur sa machine.

Les tests comparent le son rendu des deux côtés, conservent l’écoute indépendante et rouvrent
une copie enregistrée après la déconnexion.

## Ce qu’on fait maintenant

**Code en vérification : publication de prises terminées par l’hôte.**
Les essais ciblés passent, dont un enregistrement natif avec micro simulé commencé après
la connexion, sa publication, la conservation des originaux et la sauvegarde/réouverture.
La suite complète est réussie. La compilation et le paquet de ce nouveau palier restent à terminer.
Le code du bouton, les vérifications de fichiers et le raccordement aux lecteurs sont en place.
Les tests ciblés incluent le transfert réel, le son identique, le refus des fichiers incorrects,
la conservation des originaux et la sauvegarde/réouverture.

**Prochain développement prévu : ajouter des pistes au projet partagé. Statut : à commencer.**
Cela évitera de devoir préparer toutes les pistes audio avant le premier partage.
L’envoi de prises par l’invité et l’enregistrement directement dans l’onglet partagé restent à construire.

L’ordre de travail reste :

1. Terminer les vérifications et livrer le paquet de publication des prises, puis ajouter des pistes partagées. Critère : créer une piste audio pendant la connexion, y publier un son et conserver les deux projets identiques.
2. Compléter les capacités multijoueurs encore absentes : pistes, appareils, participants supplémentaires et reconnexion.
3. Stabiliser l’installation et la mise à jour de la même application pour tout le groupe.
4. Poursuivre la préparation et la performance DJ : playlists, cues, grilles, platines, export.

**L’essai sur deux ordinateurs à la maison reste pour plus tard, comme tu l’as demandé.**
Son créneau sera décidé avec toi ; ce suivi ne le présente pas comme déjà effectué.

## Ce qui a été vérifié

| Vérification | État constaté |
|---|---|
| Ensemble des tests automatiques sur ce Mac | **3 102 réussis, aucun échec, 53 ignorés**, pour le nouveau code de publication. |
| Deux sessions reliées par une vraie connexion chiffrée sur ce Mac | **Réussi pour le MIDI et le nouveau transfert audio**, avec égalité du son rendu, lecteurs indépendants et sauvegarde/réouverture. |
| Boutons du panneau multijoueur et conservation de l’original | **Réussi dans les tests d’interface.** Affichage également contrôlé visuellement. |
| Paquet Mac Apple Silicon avec audio `9a6b7762` | **Réussi.** Les trois exécutables sont compilés ; signature locale, extraction ordinaire, version de l’outil et sommes de contrôle vérifiées. |
| Contrôle GitHub Linux du code audio précédent `9a6b7762` | **Réussi pour la version précédente `9a6b7762`. Le nouveau code de publication attend sa propre validation.** |
| Contrôle GitHub Windows du code audio précédent `9a6b7762` | **Réussi pour la version précédente `9a6b7762`. Le nouveau code de publication attend sa propre validation.** |
| Contrôle GitHub Mac Universal du code audio précédent `9a6b7762` | **Réussi pour la version précédente `9a6b7762`. Le nouveau code de publication attend sa propre validation.** |
| Deux ordinateurs physiques | **Pas encore testé.** |

[Contrôles GitHub du code audio livré](https://github.com/Nozomagarion/suhina-daw/actions/runs/40419500365),
révision complète `9a6b776288ea58b87d09e6af9d6a4a6519e96600` : les trois plateformes sont réussies, constaté le 27 octobre 2026 à 10 h 01. Cette version précède la publication de nouvelles prises.
Les trois contrôles de [l’ancienne version MIDI `65d031f8`](https://github.com/Nozomagarion/suhina-daw/actions/runs/40387496818)
sont réussis ; ce résultat ne valide pas le nouveau code audio.
La validation sur deux ordinateurs et la publication d’une version commune restent à faire.
Les états ci-dessus sont un relevé daté ; ce fichier ne se rafraîchit pas tout seul.

## Où trouver la version actuelle

- **Code et présentation :** [notre dépôt GitHub](https://github.com/Nozomagarion/suhina-daw).
- **Branche de notre application :** `feature/multiplayer`, affichée dès l’accueil du dépôt.
- **Paquet local :** dans le dossier `builds` voisin du dépôt,
  `suhina-multiplayer-audio-macos-arm64-9a6b7762.zip` (**52,3 Mio**).
- **Dans le paquet :** `Suhina.app`, les outils nécessaires et les sources du commit livré.
- **Somme de contrôle :** fichier `suhina-multiplayer-audio-macos-arm64-9a6b7762.zip.sha256`, à côté du ZIP.
- **Preuves locales :** dossier `validation/2026-10-27` voisin du dépôt : journaux de tests,
  compilation, vérification du paquet et captures du panneau.

Le paquet précédent indiqué ici utilise le protocole 5. Le nouveau code de publication utilise le protocole 6 : les deux participants devront installer le nouveau paquet une fois livré.
L’ancien paquet MIDI utilise le protocole 4 et ne peut pas rejoindre une session audio de cette version.
Le canal commun de mises à jour n’est pas encore publié. Les nouveautés Suhina/rbxport doivent
être intégrées à notre version, testées, puis distribuées ensemble pour conserver le multijoueur.

## Comment ce suivi sera tenu

**Ce fichier devient le point de référence.** À chaque étape de développement terminée,
ou si une priorité ou un blocage change, il sera mis à jour avec : le changement concret,
sa raison, ce qui a été vérifié et la prochaine étape. Les réponses dans la conversation
résumeront ces mêmes points et renverront ici.

Les anciens fichiers `AUDIT_…` restent des comptes rendus techniques historiques.
Ils décrivent parfois une ancienne version : utilise ce fichier pour l’état d’ensemble.
Aucun pourcentage global du « logiciel fini » n’est annoncé ; le [suivi détaillé](fork-progress.json)
compte seulement les étapes listées, de tailles différentes.
