# Où en est notre application ?

**Bilan local vérifié le 27 octobre 2026 à 00 h 20 (heure de Paris).**
Ce bilan décrit le nouveau code de partage audio. La compilation du nouveau paquet est la
dernière opération de livraison ; la référence du paquet sera ajoutée une fois vérifiée.

## À retenir

On construit **une seule application pour produire, créer à plusieurs, préparer son set et jouer**.
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport.

**Nouveau palier : les sons déjà chargés dans le studio peuvent maintenant être transférés à l’invité.**
Les tests ciblés et la suite complète passent ; le nouveau paquet est en préparation.
La prochaine étape de musique partagée sera la publication des **nouvelles prises enregistrées** pendant la session.

Les nouvelles fonctions sont dans un **paquet de développement compilé**. Les dernières étapes
n’ont pas remplacé automatiquement l’application déjà installée ou ouverte sur ton Mac.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à deux** | Un hôte crée une session, un invité la rejoint sur le réseau local. Une copie MIDI/audio peut être partagée avec les sons déjà chargés, en conservant l’original. | Partager les plugins, ajouter de nouveaux sons pendant la connexion et accueillir plus d’un invité. |
| **Notes MIDI** | Ajouter, modifier et supprimer les notes ; régler leur hauteur, durée et vélocité. | Étendre les outils partagés et l’édition des instruments. |
| **Clips MIDI** | Créer, déplacer, changer les bords, dupliquer, activer et supprimer les clips MIDI/audio de l’arrangement. | Partager davantage d’opérations du studio, dont l’ajout de pistes et les déplacements entre cases du lanceur. |
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
dans la copie initiale ; les nouvelles prises pendant une session ne sont pas encore publiées.

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

**Code terminé et validé localement : transfert des sons présents au début de la session.**
**Livraison en cours : compilation et vérification du nouveau paquet.**
Le code du bouton, les vérifications de fichiers et le raccordement aux lecteurs sont en place.
Les tests ciblés incluent le transfert réel, le son identique, le refus des fichiers incorrects,
la conservation des originaux et la sauvegarde/réouverture.

**Prochain développement prévu : partager les nouvelles prises pendant la session. Statut : à commencer.**
Cela devra permettre d’enregistrer une nouvelle prise et de la rendre disponible à l’autre
participant sans recréer toute la session.

L’ordre de travail reste :

1. Finir la livraison du paquet de ce premier partage audio validé localement.
2. Partager les nouvelles prises enregistrées pendant le travail commun.
3. Compléter les capacités multijoueurs encore absentes : pistes, appareils, participants supplémentaires et reconnexion.
4. Stabiliser l’installation et la mise à jour de la même application pour tout le groupe.
5. Poursuivre la préparation et la performance DJ : playlists, cues, grilles, platines, export.

**L’essai sur deux ordinateurs à la maison reste pour plus tard, comme tu l’as demandé.**
Son créneau sera décidé avec toi ; ce suivi ne le présente pas comme déjà effectué.

## Ce qui a été vérifié

| Vérification | État constaté |
|---|---|
| Ensemble des tests automatiques sur ce Mac | **3 093 réussis, aucun échec ; 53 facultatifs ignorés.** Le total comprend le socle Suhina et nos ajouts. |
| Deux sessions reliées par une vraie connexion chiffrée sur ce Mac | **Réussi pour le MIDI et le nouveau transfert audio**, avec égalité du son rendu, lecteurs indépendants et sauvegarde/réouverture. |
| Boutons du panneau multijoueur et conservation de l’original | **Réussi dans les tests d’interface.** Affichage également contrôlé visuellement. |
| Paquet Mac Apple Silicon avec audio | **En préparation.** Le précédent paquet MIDI reste disponible ci-dessous. |
| Contrôle GitHub Linux de la version précédente `65d031f8` | **Réussi ; le nouveau code audio doit encore passer sur GitHub.** |
| Contrôle GitHub Windows de la version précédente `65d031f8` | **Réussi ; le nouveau code audio doit encore passer sur GitHub.** |
| Contrôle GitHub Mac Universal de la version précédente `65d031f8` | **Réussi ; le nouveau code audio doit encore passer sur GitHub.** |
| Deux ordinateurs physiques | **Pas encore testé.** |

[Contrôles GitHub de la version décrite](https://github.com/Nozomagarion/suhina-daw/actions/runs/40387496818).
Les trois contrôles de l’ancienne version sont réussis. Ceux du nouveau code audio ne sont
pas déduits de ces résultats. La validation en conditions
réelles sur deux ordinateurs et la publication d’une version commune restent à faire.
Les états ci-dessus sont un relevé daté ; ce fichier ne se rafraîchit pas tout seul.

## Où trouver la version actuelle

- **Code et présentation :** [notre dépôt GitHub](https://github.com/Nozomagarion/suhina-daw).
- **Branche de notre application :** `feature/multiplayer`, affichée dès l’accueil du dépôt.
- **Paquet local :** dans le dossier `builds` voisin du dépôt,
  `suhina-multiplayer-clips-macos-arm64-65d031f8.zip`.
- **Dans le paquet :** `Suhina.app`, les outils nécessaires et les sources du commit livré.

Le paquet MIDI indiqué ci-dessus utilise le protocole 4. Le nouveau transfert audio utilise
le protocole 5 : les deux participants devront installer la nouvelle version une fois livrée.
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
