# Où en est notre application ?

**Bilan du 28 octobre 2026 — ajout et retrait de FM Synth/Utility validés localement.**
Les deux participants peuvent maintenant construire leur chaîne sonore pendant le partage,
dans **Multiplayer → Devices**, sans recréer le morceau ni perdre les prises.
**3 144 tests passent, aucun échec.** Le nouveau paquet Mac est en préparation ; le dernier
paquet livré reste `09edc15b`, avec les réglages des appareils existants.

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
| **Pistes et mixeur** | Les deux participants ajoutent des pistes audio ou MIDI/FM Synth ; noms, volumes et panoramiques partagés. | Suppression, réorganisation et groupes. |
| **Appareils et sons** | Ajouter, retirer et régler FM Synth et Utility depuis l’un ou l’autre participant. | Réorganisation, activation/désactivation, autres effets/instruments et plugins. |
| **Nouvelles prises** | Les deux participants publient un son terminé depuis un onglet Studio local ; l’hôte vérifie les envois. | Enregistrement directement dans l’onglet partagé, publication automatique et rendu des effets/découpes. |
| **Écoute personnelle** | Chacun garde son lecteur, sa position, son démarrage et son arrêt. | Essai sur les deux ordinateurs physiques. |
| **Espace DJ** | Bibliothèque persistante, import, analyse tempo/tonalité issue de rbxport, ajout d’un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

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

**Les deux applications doivent utiliser le nouveau protocole 10**, donc le même nouveau paquet.
Les versions précédentes en protocole 9 ou moins sont incompatibles avec ces sessions.
Les projets sauvegardés gardent leur format natif version 3 ; ce changement concerne la connexion.

## Ce qui a été vérifié pour ce palier

| Vérification | Résultat |
|---|---|
| Ensemble des tests sur ce Mac | **3 144 réussis, aucun échec**, dans 130 suites ; 57 contrôles non lancés dans le passage standard. Ce total comprend Suhina et nos ajouts. |
| Commandes natives | **9 tests ciblés réussis** : placement, retrait, paramètres, annulation, sauvegarde, refus d’instrument sur piste audio, états opaques, quotas et conservation des notes/entrées/identités. |
| Réseau et collaboration | **72 tests réseau et 20 tests du cœur réussis**, dont vrais échanges TLS dans les deux sens, doublons et brouillons périmés refusés. |
| Audio et prises | **Réussi** : mêmes projets et rendus, prises publiées et conservées malgré les changements de chaîne, lecteurs à des positions différentes et comportement sonore identique au studio natif. |
| Retour de l’invité | **Réussi** : chaîne et sons ajoutés pendant son absence reçus dans la nouvelle copie ; ancienne copie et original conservés. |
| Interface | **14 tests réussis** : boutons réellement cliqués, sélection annulée sur page fermée, décalage d’index, plusieurs Utility, réutilisation d’une ancienne place et chaîne vide. Captures vérifiées séparément. |
| Qualité du code et documentation | **Réussi** : contrôle Rust strict, format, sources, documentation et six tests Python de distribution ; pas de nouvelle dépendance. |
| Nouveau paquet Mac Apple Silicon | En préparation ; l’application installée ou déjà ouverte n’a pas été remplacée. |
| Deux ordinateurs physiques | **Pas encore testé**, comme convenu. |

## Où trouver la version actuelle

Le nouveau paquet est en préparation. Le dernier paquet déjà livré reste
`suhina-multiplayer-devices-macos-arm64-09edc15b.zip`, dans `builds` voisin du dépôt.
Il utilise le protocole 9 et ne contient pas encore l’ajout/retrait d’appareils.

## GitHub et distribution

Notre branche **`feature/multiplayer`** est affichée à l’accueil du
[dépôt GitHub](https://github.com/Nozomagarion/suhina-daw). `main` reste le miroir de Suhina.
Le README et ses barres présentent notre application : **14 étapes réalisées sur 17 listées
pour créer ensemble**, avec les preuves dans `fork-progress.json`. Ce n’est pas un pourcentage
du logiciel final ; l’ajout/retrait des appareils est distingué du réglage d’un appareil existant.

**Blocage externe à nouveau constaté le 27 octobre à 23 h 44**, sur le code
`09edc15b3d1dd547a757a1f3631fd6bdda325303` :
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
invité conserve la session des autres. Les quotas et les permissions restent contrôlés.

Ensuite : reconnexion plus complète et Internet, version et mise à jour communes, puis
préparation/performance DJ avancées. L’essai des deux ordinateurs à la maison reste différé,
comme demandé.

Ce fichier est le point de référence à chaque palier ou blocage. Les anciens fichiers
`AUDIT_…` restent historiques. Les résultats externes sont des relevés datés ; ce document
n’implique pas de surveillance ou de travail automatiques entre nos échanges.
