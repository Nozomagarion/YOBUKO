# Où en est notre application ?

**Bilan du 28 octobre 2026 — lanceur de clips et scènes partagés livrés et validés localement.**
Dans **Multiplayer → Launcher**, les deux participants organisent la grille, copient leurs clips
et préparent les scènes, tout en gardant leurs lancements et leur écoute personnels.
**3 155 tests passent, aucun échec.** Le nouveau code utilise le protocole 11.
**Le paquet Mac `fdf6eab9` est compilé et vérifié après extraction**, avec les trois exécutables
et les sources exactes. Le code et ce suivi sont envoyés sur notre GitHub. Les contrôles
distants restent bloqués par GitHub ; Windows/Linux ne sont pas validés pour cette version.

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
| **Notes, clips et scènes** | Modifier les notes et l’arrangement ; créer/copier/déplacer/échanger/effacer les cases du lanceur et organiser les scènes ensemble. | Davantage d’outils partagés, dont le retour du lanceur vers l’arrangement. |
| **Pistes et mixeur** | Les deux participants ajoutent des pistes audio ou MIDI/FM Synth ; noms, volumes et panoramiques partagés. | Suppression, réorganisation et groupes. |
| **Appareils et sons** | Ajouter, retirer et régler FM Synth et Utility depuis l’un ou l’autre participant. | Réorganisation, activation/désactivation, autres effets/instruments et plugins. |
| **Nouvelles prises** | Les deux participants publient un son terminé depuis un onglet Studio local ; l’hôte vérifie les envois. | Enregistrement directement dans l’onglet partagé, publication automatique et rendu des effets/découpes. |
| **Écoute personnelle** | Chacun garde son lecteur, sa position, son démarrage et son arrêt. | Essai sur les deux ordinateurs physiques. |
| **Espace DJ** | Bibliothèque persistante, import, analyse tempo/tonalité issue de rbxport, ajout d’un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

## Nouveau : préparer des scènes ensemble

Dans **Multiplayer → Launcher**, une scène est une ligne de la grille ; une case contient un
clip sur une piste. L’organisation est commune, mais chacun choisit ce qu’il lance et écoute.

1. Cliquer une case vide, choisir **Source clip**, puis **Copy clip into shared slot**. Le clip de l’arrangement reste intact ; ses notes copiées sont modifiables séparément. Un son audio déjà partagé est réutilisé.
2. Pour partir de zéro, choisir **Create empty shared slot**. Ajouter ensuite ses notes dans **MIDI notes**. Une case audio créée vide reste silencieuse tant qu’elle n’a pas de son.
3. Cliquer une ligne de scène, saisir **Scene name**, puis **Rename shared scene**, **Insert scene before**, **Duplicate shared scene** ou **Delete shared scene**. **Append shared scene** ajoute une ligne à la fin.
4. Cliquer une case remplie, choisir la destination, puis **Move or swap shared slot**. Une destination occupée compatible échange les deux clips. **Clear shared slot** retire seulement le clip de cette case.
5. **Launch slot locally** et **Launch scene locally** lancent seulement sur cet ordinateur. **Stop locally** reste personnel. Ces actions n’envoient pas un démarrage à l’autre participant.
6. Après une modification commune, **Refresh launcher draft** efface le brouillon périmé. Après insertion, duplication ou suppression d’une scène, il faut sélectionner à nouveau la ligne/case, même si la page était fermée.

Copier dans une case remplie est refusé ; aucune case voisine n’est écrasée. La dernière scène
ne peut pas être supprimée. Maximum **128 scènes**, avec les budgets communs de **512 contenus**,
**4 096 clips** et **16 384 notes**. Les patterns routés explicitement vers une ou plusieurs
pistes ne sont pas copiables dans une case à ce palier. Les clips ordinaires MIDI/audio le sont.
Les copies, scènes et cases passent par la validation de l’hôte ; chacun sauvegarde sa copie.

## Nouveau : construire la chaîne sonore ensemble

1. Créer ou rejoindre le partage, puis **Multiplayer → Devices**.
2. Choisir la piste dans **Device track**.
3. Cliquer **Add FM Synth** ou **Add Utility**. FM Synth est disponible sur une piste MIDI sans instrument ; il ne remplace jamais un instrument existant. Utility s’ajoute en fin de chaîne.
4. Choisir explicitement l’appareil dans **Shared device** pour accéder à ses réglages.
5. Pour le retirer, cliquer **Remove selected device**. Ses réglages et son automatisation sont retirés avec lui ; pistes, clips, notes et sons collectés sont conservés.
6. Après un ajout ou retrait, choisir à nouveau **Shared device**. Les anciennes sélections sont annulées, même si un nouvel appareil réutilise leur ancienne place.

**Refresh chain draft** actualise le brouillon d’ajout après une modification du morceau.
**Refresh device draft** actualise un réglage ou un retrait périmé ; après un changement de
chaîne, il faut choisir l’appareil à nouveau plutôt que réutiliser l’ancien brouillon.
Les originaux restent dans leurs onglets. Les lecteurs et leurs positions ne sont pas remplacés.
Comme dans le studio, le nouvel instrument prend le relais aux prochaines notes. Retirer FM Synth
conserve les notes et laisse la piste utiliser son son de base.

## Régler le son ensemble

1. Créer ou rejoindre un partage, puis ouvrir **Multiplayer → Devices**.
2. Choisir un FM Synth ou Utility existant dans **Shared device**.
3. Retrouver un paramètre avec **Find parameter**, puis le choisir dans **Shared parameter**.
4. Modifier **Draft value** avec le contrôle du studio : curseur, nombre, choix ou interrupteur selon le paramètre. Cela ne change encore que le brouillon local.
5. Cliquer **Apply shared parameter**. Après confirmation de l’hôte, **Confirmed value** et les deux projets sont mis à jour.
6. Si le morceau change entre-temps, **Apply** est désactivé : **Refresh device draft** recharge la valeur confirmée avant de préparer un nouvel envoi.

Pour essayer depuis la démonstration : **Create LAN session → Devices**, choisir **Pulse**
dans **Device track**, puis **Add FM Synth**. Ajouter Utility de la même manière, avec un
rafraîchissement du brouillon de chaîne si nécessaire.

Les contrôles reprennent les paramètres et plages du studio. Les valeurs hors plage, choix
fractionnaires, appareils non pris en charge et brouillons périmés sont refusés. Un appareil
éteint reste éteint : son réglage est conservé sans le réactiver. Comme dans le studio, régler
manuellement un paramètre désactive l’automatisation de ce paramètre, sans supprimer ses points.
Les lecteurs et positions d’écoute restent personnels. Chacun sauvegarde sa copie explicitement.

## Revenir après une coupure

1. L’invité quitte la session ou perd sa connexion. Sa dernière copie reste ouverte et redevient modifiable en solo.
2. L’hôte garde sa session ouverte et peut continuer à modifier le morceau, les réglages, les pistes et les sons.
3. Quand l’hôte ne voit plus **Guest connected**, il clique **New invitation**, puis **Copy invitation**.
4. L’invité utilise cette nouvelle invitation dans **Join LAN session**.
5. Un **nouvel onglet partagé** reçoit le morceau actuel, ses réglages et ses sons vérifiés. L’ancienne copie reste ouverte avec les éventuelles retouches solo.

Le bouton est désactivé tant qu’un invité est connecté. La nouvelle invitation invalide
l’ancienne et reste utilisable **une fois, pendant cinq minutes**. Il faut la copier à nouveau.
L’hôte garde son onglet et son lecteur. **Les retouches solo ne sont pas fusionnées** dans
le morceau partagé. Si l’hôte quitte lui-même la session, ce partage se termine.

## Comment créer et publier ensemble

1. Ouvrir le projet dans **Studio**, puis **Multiplayer → Share project + audio** pour inclure les sons chargés. **Share current MIDI project** et la démonstration **Create LAN session** restent disponibles.
2. L’hôte utilise **Copy invitation** ; l’invité colle cette invitation dans **Join LAN session**.
3. Dans **Tracks**, saisir **New track name**, choisir **Audio track** ou **MIDI track · FM Synth**, puis **Add shared track**.
4. Utiliser **Clips** pour l’arrangement, **MIDI notes** pour les notes et **Devices** pour les réglages. Un brouillon périmé doit être rafraîchi avant son envoi.
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
- FM Synth et Utility avec paramètres simples ; plugins externes, autres instruments et états opaques refusés. Pas encore de réorganisation, d’activation/désactivation ni de modification des courbes d’automatisation en commun.
- Les pistes créées n’activent aucun micro, entrée MIDI ou monitoring local. Pas d’enregistrement direct dans l’onglet partagé ni de diffusion audio en direct entre ordinateurs.

**Les deux applications doivent utiliser le nouveau protocole 11**, donc le même nouveau paquet.
Les versions précédentes en protocole 10 ou moins sont incompatibles avec ces sessions.
Les projets sauvegardés gardent leur format natif version 3 ; ce changement concerne la connexion.

## Ce qui a été vérifié pour ce palier

| Vérification | Résultat |
|---|---|
| Ensemble des tests sur ce Mac | **3 155 réussis, aucun échec**, dans 130 suites ; 58 contrôles non lancés dans le passage standard. Ce total comprend Suhina et nos ajouts. |
| Nouvelles commandes natives | **4 tests réussis** : copie indépendante, annulation, sauvegarde/réouverture, refus des cases remplies ou incompatibles, des indices invalides et des contenus à routage explicite. |
| Réseau et collaboration | **77 tests réseau et 21 tests du cœur réussis**, dont les échanges TLS réels dans les deux sens, brouillons périmés et budgets dépassés. Un refus ne consomme pas la modification suivante. |
| Notes et audio | **Réussi** : les notes copiées sont indépendantes, l’arrangement original est conservé et les clips audio réutilisent leur son vérifié. Les fichiers collectés restent présents après les suppressions de scènes. |
| Retour de l’invité | **Réussi** : la nouvelle copie retrouve la grille, les scènes, les notes et la révision actuelles après renouvellement de l’invitation. |
| Interface | **15 tests multijoueur réussis**, dont les vrais boutons de la grille, sélections annulées sur page fermée et lancements/arrêt locaux sans modifier le lecteur distant. |
| Rendu visuel | **Réussi séparément** : capture GPU de la nouvelle page vérifiée. |
| Qualité du code et documentation | **Réussi** : contrôle Rust strict, format, sources, documentation et six tests Python de distribution ; aucune nouvelle dépendance. |
| Paquet Mac Apple Silicon `fdf6eab9` | **Compilé et vérifié après extraction** : signature locale, trois exécutables, version de l’outil, sources exactes et sommes de contrôle. |
| Contrôles GitHub Mac/Windows/Linux | **Non lancés : GitHub refuse le déclenchement. Aucun résultat distant annoncé pour cette révision.** |
| Deux ordinateurs physiques | **Pas encore testé**, comme convenu. |

## Où trouver la version actuelle

- **Paquet local :** dossier `builds` voisin du dépôt, `suhina-multiplayer-launcher-macos-arm64-fdf6eab9.zip`.
- **Dans le paquet :** `Suhina.app`, les trois exécutables nécessaires, ce suivi, les crédits et les sources exactes du commit livré.
- **Somme de contrôle :** fichier `.zip.sha256` à côté du ZIP ; les fichiers extraits sont vérifiés dans `SHA256SUMS.txt`.
- **Preuves locales :** dossier `validation/2026-10-28` voisin du dépôt : journaux, captures et script de fabrication du paquet.

Les tests et le paquet correspondent au code **`fdf6eab9db63a37115ef840db87b27d57b5c5e7c`**,
compilé après son enregistrement puis envoyé sur notre branche GitHub. C’est un paquet de
développement Mac Apple Silicon, signé localement. L’application installée ou ouverte n’a pas
été remplacée. Les deux participants doivent utiliser ce même nouveau paquet.

## GitHub et distribution

Notre branche **`feature/multiplayer`** est affichée à l’accueil du
[dépôt GitHub](https://github.com/Nozomagarion/suhina-daw). `main` reste le miroir de Suhina.
Le README et ses barres présentent notre application : **15 étapes réalisées sur 18 listées
pour créer ensemble**, avec les preuves dans `fork-progress.json`. Ce n’est pas un pourcentage
du logiciel final ; le nouveau palier correspond à la grille et aux scènes partagées.

**Blocage externe à nouveau constaté le 28 octobre à 9 h 48 (heure de Paris)**, sur le code
`fdf6eab9db63a37115ef840db87b27d57b5c5e7c` :
GitHub refuse le lancement des contrôles avec `HTTP 422: Actions has been disabled for this repository.`
La lecture du réglage indique pourtant `enabled: true` ; la cause de ce désaccord n’est pas établie.
Le workflow de notre fork est également indiqué actif. Aucune exécution n’était visible pour
cette révision au moment du contrôle. Le lancement manuel a échoué ; aucun résultat distant
réussi n’est annoncé pour les nouveautés. Le réglage général du dépôt n’a pas été modifié.

La version `9a6b7762` avait terminé ses [contrôles Mac, Windows et Linux](https://github.com/Nozomagarion/suhina-daw/actions/runs/40419500365),
constaté le 27 octobre à 10 h 01. Cela ne valide pas les ajouts ultérieurs.

Le canal commun de mises à jour n’est pas encore publié. Les nouveautés Suhina/rbxport doivent
être intégrées à notre code, testées puis distribuées ensemble pour conserver le multijoueur.
Le bouton des paramètres compare les versions ; il n’installe pas séparément les logiciels officiels.

## Prochaine étape

**Inviter plusieurs participants sur le réseau local. Statut : à commencer.**
Critère : un hôte et au moins deux invités créent ensemble dans le même morceau, chacun avec
son invitation privée, ses modifications validées et son écoute indépendante ; le départ d’un
invité conserve la session des autres. Les quotas et permissions restent contrôlés.

Ensuite : reconnexion plus complète et Internet, version et mise à jour communes, puis
préparation/performance DJ avancées. L’essai des deux ordinateurs à la maison reste différé,
comme demandé.

Ce fichier est le point de référence à chaque palier ou blocage. Les anciens fichiers
`AUDIT_…` restent historiques. Les résultats externes sont des relevés datés ; ce document
n’implique pas de surveillance ou de travail automatiques entre nos échanges.
