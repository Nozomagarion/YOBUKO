# Où en est notre application ?

**Bilan vérifié le 26 octobre 2026 à 21 h 19 (heure de Paris).**
Version du logiciel décrite : `65d031f8` — la dernière étape de code livrée.

## À retenir

On construit **une seule application pour produire, créer à plusieurs, préparer son set et jouer**.
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport.

**Aujourd’hui : un prototype MIDI à deux fonctionne dans les tests, avec une première bibliothèque DJ.**
L’application complète que tu veux reste en construction. Le prochain gros morceau est de
permettre aux participants de recevoir les **fichiers audio** du projet.

Les nouvelles fonctions sont dans un **paquet de développement compilé**. Les dernières étapes
n’ont pas remplacé automatiquement l’application déjà installée ou ouverte sur ton Mac.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à deux** | Un hôte crée une session, un invité la rejoint sur le réseau local. Une copie d’un projet MIDI est partagée en conservant l’original. | Partager les projets contenant de l’audio et des plugins ; accueillir plus d’un invité. |
| **Notes MIDI** | Ajouter, modifier et supprimer les notes ; régler leur hauteur, durée et vélocité. | Étendre les outils partagés et l’édition des instruments. |
| **Clips MIDI** | Créer, déplacer, changer les bords, dupliquer, activer et supprimer les blocs de musique de l’arrangement. | Partager davantage d’opérations du studio, dont l’ajout de pistes et les déplacements entre cases du lanceur. |
| **Écoute personnelle** | Chacun peut écouter une autre partie, démarrer et arrêter sa propre lecture. | Vérifier ce comportement sur les deux ordinateurs de la maison. |
| **Espace DJ** | Importer des morceaux dans une bibliothèque conservée sur le disque, analyser tempo et tonalité avec du code de rbxport, y ajouter un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Un bouton dans les paramètres compare les versions GitHub. Les outils suivent les deux projets d’origine et vérifient les paquets de notre application. | Activer l’intégration régulière des nouveautés, leur publication et la réception automatique d’une version commune par tous. |

Le travail à deux passe actuellement par le panneau **Multiplayer**, avec les onglets
**Mixer**, **MIDI notes** et **Clips**. L’édition partagée couvre ces commandes précises.

**Limite importante :** la copie partagée accepte actuellement les projets MIDI compatibles
avec FM Synth et Utility, sans état d’appareil opaque. Un projet contenant des pistes audio,
d’autres instruments ou des plugins externes est refusé pour le moment.
Les voix, samples et enregistrements ne circulent pas encore entre les participants.

## Ce qu’on vient de fabriquer, et pourquoi

1. **Partager ton propre projet MIDI.** Pour travailler sur ta musique, au-delà de la démonstration.
2. **Modifier les notes à deux.** Pour pouvoir composer ensemble.
3. **Modifier les clips à deux.** Pour pouvoir arranger le morceau ensemble, pas seulement changer un volume.
4. **Protéger les modifications.** Si quelqu’un a déjà changé le passage, un vieux brouillon est refusé.
   Déplacer ou agrandir un clip conserve ses voisins. Éditer une duplication protège son original.

Ces étapes posent le fonctionnement commun avant d’y ajouter des fichiers audio et des prises enregistrées.

## Ce qu’on fait maintenant

**Ce point de travail : mettre en place ce suivi unique et clarifier l’état réel.**
La dernière étape de code, l’arrangement MIDI partagé, est terminée et livrée.

**Prochain développement prévu : le partage des fichiers audio. Statut : à commencer.**
Le résultat attendu est concret : l’invité doit recevoir les sons utilisés par le projet,
même s’ils n’existent pas encore sur son ordinateur, puis pouvoir les écouter.
On devra aussi gérer l’interruption du transfert et éviter qu’un fichier incomplet soit utilisé.

L’ordre de travail reste :

1. Partager les fichiers audio nécessaires au morceau.
2. Partager les nouvelles prises enregistrées pendant le travail commun.
3. Compléter les capacités multijoueurs encore absentes : pistes, appareils, participants supplémentaires et reconnexion.
4. Stabiliser l’installation et la mise à jour de la même application pour tout le groupe.
5. Poursuivre la préparation et la performance DJ : playlists, cues, grilles, platines, export.

**L’essai sur deux ordinateurs à la maison reste pour plus tard, comme tu l’as demandé.**
Son créneau sera décidé avec toi ; ce suivi ne le présente pas comme déjà effectué.

## Ce qui a été vérifié

| Vérification | État constaté |
|---|---|
| Ensemble des tests automatiques sur ce Mac | **3 079 réussis, aucun échec ; 52 facultatifs ignorés.** Le total comprend le socle Suhina et nos ajouts. |
| Deux sessions reliées par une vraie connexion chiffrée sur ce Mac | **Réussi** pour les fonctions MIDI décrites, avec lecteurs audio indépendants et vérification du son rendu. |
| Boutons du panneau multijoueur et conservation de l’original | **Réussi dans les tests d’interface.** Affichage également contrôlé visuellement. |
| Paquet Mac Apple Silicon | **Compilé et vérifié après extraction**, avec les trois programmes et les sources correspondantes. |
| Dernier contrôle GitHub Linux | **Réussi.** |
| Dernier contrôle GitHub Windows | **Réussi.** |
| Dernier contrôle GitHub Mac Universal | **Encore en cours lors de cette vérification.** |
| Deux ordinateurs physiques | **Pas encore testé.** |

[Contrôles GitHub de la version décrite](https://github.com/Nozomagarion/suhina-daw/actions/runs/40387496818).
Les états ci-dessus sont un relevé daté ; ce fichier ne se rafraîchit pas tout seul.

## Où trouver la version actuelle

- **Code et présentation :** [notre dépôt GitHub](https://github.com/Nozomagarion/suhina-daw).
- **Branche de notre application :** `feature/multiplayer`, affichée dès l’accueil du dépôt.
- **Paquet local :** dans le dossier `builds` voisin du dépôt,
  `suhina-multiplayer-clips-macos-arm64-65d031f8.zip`.
- **Dans le paquet :** `Suhina.app`, les outils nécessaires et les sources du commit livré.

Les deux participants devront utiliser une version compatible ; celle-ci utilise le protocole LAN 4.
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
