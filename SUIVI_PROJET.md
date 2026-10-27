# Où en est notre application ?

**Bilan du 27 octobre 2026 — réinvitation livrée et validée localement.**
Le bouton **New invitation** conserve le morceau ouvert chez l’hôte. L’invité revient dans
une nouvelle copie à jour ; son ancienne copie et ses retouches locales restent conservées.
**3 126 tests passent, aucun échec.** Le paquet Mac **`b3967616`** est compilé et vérifié
après extraction. Le code et ce suivi sont envoyés sur notre GitHub. Les contrôles distants
restent bloqués par GitHub ; Windows/Linux ne sont donc pas validés pour cette version.

## Ce qu’on fabrique

**Une seule application pour produire, créer à plusieurs, préparer son set et jouer.**
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport. Les auteurs et les licences restent crédités.

La priorité reste de rendre le travail musical à plusieurs plus complet. Le test sur les deux
ordinateurs de la maison est prévu plus tard, comme demandé. Le logiciel complet reste en développement.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à deux** | Un hôte et un invité sur le réseau local ; partager une copie MIDI/audio avec les sons chargés, en conservant les originaux. | Plus d’un invité et Internet. |
| **Reprendre après une coupure** | L’hôte crée une nouvelle invitation sans fermer le morceau ; l’invité ouvre l’état actuel dans un nouvel onglet. | Reconnexion automatique, fusion des retouches hors connexion et reprise après arrêt de l’hôte. |
| **Notes et clips** | Modifier les notes MIDI ; créer, déplacer, redimensionner, dupliquer, activer et supprimer les clips de l’arrangement. | Édition des cases du lanceur et davantage d’outils partagés. |
| **Pistes et mixeur** | Les deux participants ajoutent des pistes audio ou MIDI/FM Synth ; noms, volumes et panoramiques partagés. | Suppression, réorganisation/groupes et modification commune des appareils. |
| **Nouvelles prises** | Les deux participants publient un son terminé depuis un onglet Studio local ; l’hôte vérifie les envois. | Enregistrement directement dans l’onglet partagé, publication automatique et rendu des effets/découpes. |
| **Écoute personnelle** | Chacun garde son lecteur, sa position, son démarrage et son arrêt. | Essai sur les deux ordinateurs physiques. |
| **Espace DJ** | Bibliothèque persistante, import, analyse tempo/tonalité issue de rbxport, ajout d’un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

## Nouveau : revenir sans recréer le morceau

1. L’invité quitte la session ou perd sa connexion. Sa dernière copie reste ouverte et redevient modifiable en solo.
2. L’hôte garde sa session ouverte et peut continuer à modifier le morceau, ajouter des pistes ou publier des sons.
3. Quand l’hôte ne voit plus **Guest connected**, il clique **New invitation**, puis **Copy invitation**.
4. L’invité utilise cette nouvelle invitation dans **Join LAN session**.
5. Un **nouvel onglet partagé** reçoit le morceau actuel et les sons vérifiés. L’ancienne copie de l’invité reste ouverte, avec ses éventuelles retouches solo.

Le bouton est désactivé tant qu’un invité est connecté. Une nouvelle invitation invalide
l’ancienne et reste utilisable **une fois, pendant cinq minutes**. Il faut la copier à nouveau.
L’hôte garde son onglet, son lecteur, les pistes, les sons et l’ordre des modifications.
Aucune révision musicale n’est consommée par ce changement d’invitation.

**Les retouches solo de l’ancien onglet ne sont pas fusionnées dans le morceau partagé.**
Enregistrer explicitement les copies qu’on veut garder. Si l’hôte quitte lui-même la session,
ce partage se termine ; la reprise automatique de l’hôte n’est pas encore construite.

## Comment créer et publier ensemble

1. Ouvrir le projet dans **Studio**, puis **Multiplayer → Share project + audio** pour inclure les sons chargés. **Share current MIDI project** et la démonstration **Create LAN session** restent disponibles.
2. L’hôte utilise **Copy invitation** ; l’invité colle cette invitation dans **Join LAN session**.
3. Dans **Tracks**, saisir **New track name**, choisir **Audio track** ou **MIDI track · FM Synth**, puis **Add shared track**.
4. Utiliser **Clips** pour l’arrangement et **MIDI notes** pour les notes. Un brouillon périmé doit être rafraîchi avant d’envoyer sa modification.
5. Pour apporter une prise : l’enregistrer et la terminer dans un **autre onglet Studio local ordinaire**, puis garder Multiplayer ouvert, sélectionner cet onglet et ouvrir **Publish take**.
6. Choisir le son chargé, la piste audio partagée et **Start beat**, puis **Publish loaded take**. L’hôte vérifie la prise de l’invité avant son ajout. Chacun sauvegarde explicitement sa copie partagée.

**Preparing take…** puis **Sending take** indiquent la préparation et l’envoi. Une prise
périmée peut être refusée si le morceau change pendant le transfert : attendre la mise à jour,
puis relancer. La publication utilise le son entier, sans les découpes, boucles ou effets du
clip source. L’écoute locale continue ; les échanges de modifications peuvent attendre la fin du transfert.

Les sons collectés restent sous **Shared projects**, dans le dossier de données de l’application.
Leurs chemins locaux sont conservés pour la sauvegarde et la réouverture, sans être transmis
comme chemins de fichiers à l’autre ordinateur. Les originaux restent intacts.

## Limites actuelles

- **Un hôte et un invité, sur le réseau local**, sans serveur Internet ni reconnexion automatique.
- **64 pistes**, retours et groupes compris ; document partagé de **2 Mio** maximum.
- **64 sons uniques**, **64 Mio par son**, **256 Mio au total**, mono/stéréo. Supprimer un clip ne remet pas ce budget à zéro. Un son déjà vérifié n’est pas renvoyé inutilement.
- WAV portables à partir du son décodé ; le format MP3/FLAC d’origine et ses tags ne sont pas archivés.
- FM Synth et Utility avec paramètres simples au partage initial ; plugins externes, autres instruments et états opaques refusés. Les pistes créées n’activent aucun micro, entrée MIDI ou monitoring local.
- Pas d’enregistrement direct dans l’onglet partagé ni de diffusion audio en direct entre ordinateurs.

Les deux applications utilisent le **protocole 8**. Le renouvellement d’invitation ne change
pas ce protocole. Les anciens paquets en protocole 4, 5, 6 ou 7 sont incompatibles.

## Ce qui a été vérifié pour ce palier

| Vérification | Résultat |
|---|---|
| Ensemble des tests sur ce Mac | **3 126 réussis, aucun échec**, dans 130 suites ; 55 contrôles non lancés dans le passage standard. Ce total comprend Suhina et nos ajouts. |
| Tests du module réseau | **67 réussis** : cœur, vraies connexions TLS, audio et reprises. |
| Renouvellement | **Réussi** : invitation expirée/utilisée, ancienne invitation refusée, connexion déjà entamée, invité encore présent et file d’événements pleine ou fermée. |
| Retours successifs | **Quatre cycles réussis** : départ, modifications chez l’hôte, nouvelle invitation et nouvelles modifications par l’invité. |
| Pistes, sons et lecteur | **Réussi** : récupération des pistes et prises ajoutées pendant l’absence, publication par l’invité revenu, son identique, copie précédente et lecteur de l’hôte conservés. |
| Interface | **12 tests réussis** : bouton réellement cliqué, même onglet chez l’hôte, nouvel onglet chez l’invité et ancienne copie intacte. Contrôle visuel GPU exécuté séparément avec succès. |
| Qualité du code et documentation | **Réussi** : contrôle Rust strict, format, sources, documentation et six tests Python de distribution ; pas de nouvelle dépendance. |
| Paquet Mac Apple Silicon `b3967616` | **Compilé et vérifié après extraction** : signature locale, trois exécutables, version de l’outil, sources exactes et sommes de contrôle. |
| Contrôles GitHub Mac/Windows/Linux | **Non lancés : GitHub refuse le déclenchement. Aucun résultat annoncé pour cette révision.** |
| Deux ordinateurs physiques | **Pas encore testé**, comme convenu. |

Les tests et le paquet correspondent au code **`b39676164c6427c79e4cfa427bfa8fc658e0a1a7`**,
compilé après son enregistrement puis envoyé sur notre branche GitHub.

## Où trouver la version actuelle

- **Paquet local :** dossier `builds` voisin du dépôt, `suhina-multiplayer-reinvite-macos-arm64-b3967616.zip`.
- **Dans le paquet :** `Suhina.app`, les trois exécutables nécessaires, ce suivi, les crédits et les sources exactes du commit livré.
- **Somme de contrôle :** fichier `.zip.sha256` à côté du ZIP ; les fichiers extraits sont vérifiés dans `SHA256SUMS.txt`.
- **Preuves locales :** dossier `validation/2026-10-27` voisin du dépôt : journaux, capture du bouton et script de fabrication du paquet.

C’est un paquet de développement Mac Apple Silicon, signé localement. L’application installée
ou déjà ouverte sur ton Mac n’a pas été remplacée.

## GitHub et distribution

Le code est sur notre branche **`feature/multiplayer`**, affichée à l’accueil du
[dépôt GitHub](https://github.com/Nozomagarion/suhina-daw). `main` reste le miroir de Suhina.
Le README et ses barres présentent notre application : **12 étapes réalisées sur 15 listées
pour créer ensemble**, avec les preuves dans `fork-progress.json`. Ce n’est pas un pourcentage
du logiciel final ; les étapes ont été précisées pour distinguer réinvitation manuelle,
appareils partagés et reconnexion automatique.

**Blocage externe à nouveau constaté le 27 octobre à 21 h 57**, sur le code
`b39676164c6427c79e4cfa427bfa8fc658e0a1a7` :
GitHub refuse le lancement des contrôles avec `HTTP 422: Actions has been disabled for this repository.`
La lecture du réglage indique pourtant `enabled: true` ; la cause de ce désaccord n’est pas établie.
Aucune exécution n’était visible pour cette révision au moment du contrôle. Aucun résultat
distant réussi n’est annoncé pour les nouveautés. Le réglage général du dépôt n’a pas été modifié.

La version `9a6b7762` avait terminé ses [contrôles Mac, Windows et Linux](https://github.com/Nozomagarion/suhina-daw/actions/runs/40419500365),
constaté le 27 octobre à 10 h 01. Cela ne valide pas les ajouts ultérieurs.

Le canal commun de mises à jour n’est pas encore publié. Les nouveautés Suhina/rbxport doivent
être intégrées à notre code, testées puis distribuées ensemble pour conserver le multijoueur.
Le bouton des paramètres compare les versions ; il n’installe pas séparément les logiciels officiels.

## Prochaine étape

**Partager les réglages FM Synth et Utility pendant la session. Statut : à commencer.**
Critère : modifier un paramètre d’un appareil déjà présent depuis l’un des deux ordinateurs,
entendre la modification et retrouver le même réglage dans l’autre copie, en conservant les
lecteurs indépendants et en refusant un brouillon périmé. Les plugins externes restent une étape distincte.

Ensuite : appareils et participants supplémentaires, reconnexion plus complète, version et
mise à jour communes, puis préparation/performance DJ avancées. L’essai des deux ordinateurs
à la maison reste différé, comme demandé.

Ce fichier est le point de référence à chaque palier ou blocage. Les anciens fichiers
`AUDIT_…` restent historiques. Les résultats externes sont des relevés datés ; ce document
n’implique pas de surveillance ou de travail automatiques entre nos échanges.
