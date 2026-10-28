# Où en est notre application ?

**Bilan du 28 octobre 2026 — un hôte et trois invités livrés et validés localement.**
Quatre musiciens peuvent partager le même morceau sur le réseau local. Chacun reçoit son
invitation, conserve son lecteur et peut envoyer des modifications et des prises terminées.
Le départ d’un invité laisse la session des autres ouverte ; un compteur affiche les places.
**3 162 tests passent, aucun échec.** Le nouveau code utilise le protocole 12.
**Le paquet Mac `4be2d443` est compilé et vérifié après extraction**, avec les trois exécutables
et les sources exactes. Le code et ce suivi sont envoyés sur notre GitHub. Les contrôles distants
restent bloqués par GitHub ; Windows/Linux ne sont pas validés pour cette version.

## Ce qu’on fabrique

**Une seule application pour produire, créer à plusieurs, préparer son set et jouer.**
Suhina fournit le studio. Notre fork ajoute le multijoueur et le lien avec la préparation DJ,
qui commence avec l’analyse musicale de rbxport. Les auteurs et les licences restent crédités.

La prochaine priorité est la préparation des sets DJ, après ce palier de collaboration LAN.
Les essais sur les ordinateurs de la maison restent prévus plus tard, comme demandé. Le logiciel
complet reste en développement.

## Ce qui existe vraiment

| Partie | Ce qu’on peut faire actuellement | Ce qui manque encore |
|---|---|---|
| **Studio** | Utiliser le studio de Suhina et envoyer le rendu d’un morceau vers notre bibliothèque DJ. | Relier toutes les fonctions du studio à la collaboration. |
| **Créer à plusieurs** | Un hôte et trois invités maximum sur le réseau local ; copie MIDI/audio et sons vérifiés ; chacun modifie et publie ses prises, en conservant les originaux. | Internet, noms/rôles des participants et transfert du rôle d’hôte. |
| **Reprendre après une coupure** | L’hôte crée une nouvelle invitation sans fermer le morceau ; l’invité ouvre l’état actuel dans un nouvel onglet. | Reconnexion automatique, fusion des retouches hors connexion et reprise après arrêt de l’hôte. |
| **Notes, clips et scènes** | Modifier les notes et l’arrangement ; créer/copier/déplacer/échanger/effacer les cases du lanceur et organiser les scènes ensemble. | Davantage d’outils partagés, dont le retour du lanceur vers l’arrangement. |
| **Pistes et mixeur** | Les participants ajoutent des pistes audio ou MIDI/FM Synth ; noms, volumes et panoramiques partagés. | Suppression, réorganisation et groupes. |
| **Appareils et sons** | Ajouter, retirer et régler FM Synth et Utility depuis l’un ou l’autre participant. | Réorganisation, activation/désactivation, autres effets/instruments et plugins. |
| **Nouvelles prises** | Les participants publient un son terminé depuis un onglet Studio local ; l’hôte vérifie les envois. | Enregistrement directement dans l’onglet partagé, publication automatique et rendu des effets/découpes. |
| **Écoute personnelle** | Chacun garde son lecteur, sa position, son démarrage et son arrêt. | Essai sur les deux ordinateurs physiques. |
| **Espace DJ** | Bibliothèque persistante, import, analyse tempo/tonalité issue de rbxport, ajout d’un rendu du studio. | Playlists, points cue, grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

## Nouveau : jouer et composer à quatre sur le réseau local

Ce chantier ajoute **un hôte et jusqu’à trois invités**, donc quatre musiciens au total.
Le compteur **Guests: N / 3** indique les connexions admises, y compris les copies en cours de réception.

1. L’hôte crée le partage et utilise **Copy invitation** pour le premier invité.
2. Attendre son arrivée, puis **New invitation → Copy invitation** pour le suivant. Refaire pour le troisième. Les personnes déjà connectées restent dans le morceau.
3. À trois invités, le bouton est désactivé. Un départ libère une place sans fermer les sessions restantes ; une nouvelle invitation permet de la réutiliser.
4. Chacun peut modifier les notes, clips, scènes, pistes, sons et réglages pris en charge. Si deux modifications partent du même état du morceau, l’une peut être refusée : rafraîchir son brouillon puis la renvoyer.
5. Chaque invité peut publier une prise terminée depuis un onglet local. Si un envoi devient périmé pendant le transfert, sa source est conservée et peut être renvoyée. Les autres reçoivent le son vérifié avant son clip.
6. Chacun garde son lecteur. Le départ d’un invité conserve sa dernière copie locale ; son retour ouvre un nouvel onglet partagé sans fusionner les retouches solo. Si l’hôte quitte, le partage se termine pour tout le monde.

**Une seule invitation peut attendre à la fois.** Chaque invitation expire après cinq minutes
et fonctionne une fois. Créer la suivante remplace l’ancienne non utilisée : attendre que son
destinataire ait rejoint avant d’en créer une autre. La transmettre en privé.
Le réseau Internet, la reconnexion automatique, les noms/rôles des participants et la reprise
du rôle d’hôte après son départ restent à construire. Les essais physiques restent différés.

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

1. L’invité quitte ou perd sa connexion. Sa dernière copie reste ouverte et redevient modifiable en solo.
2. L’hôte et les autres invités restent dans le morceau et peuvent continuer à le modifier.
3. Quand le compteur indique une place libre, l’hôte utilise **New invitation**, puis **Copy invitation**.
4. L’invité utilise cette nouvelle invitation dans **Join LAN session**.
5. Un **nouvel onglet partagé** reçoit l’état actuel et ses sons vérifiés. L’ancienne copie reste ouverte avec les retouches solo éventuelles.

Le bouton est désactivé à trois invités. Une nouvelle invitation invalide seulement l’ancienne
invitation encore en attente, sans fermer les connexions déjà admises. Chaque invitation est
utilisable **une fois, pendant cinq minutes**. Les retouches solo ne sont pas fusionnées.
Si l’hôte quitte la session, le partage se termine pour tous ; les copies locales restent ouvertes.

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

- **Un hôte et trois invités maximum, sur le réseau local**, sans serveur Internet ni reconnexion automatique. Une seule invitation peut attendre à la fois.
- **64 pistes**, retours et groupes compris ; document partagé de **2 Mio** maximum.
- **64 sons uniques**, **64 Mio par son**, **256 Mio au total**, mono/stéréo. Supprimer un clip ne remet pas ce budget à zéro. Un son déjà vérifié n’est pas renvoyé inutilement.
- WAV portables à partir du son décodé ; le format MP3/FLAC d’origine et ses tags ne sont pas archivés.
- FM Synth et Utility avec paramètres simples ; plugins externes, autres instruments et états opaques refusés. Pas encore de réorganisation, d’activation/désactivation ni de modification des courbes d’automatisation en commun.
- Les pistes créées n’activent aucun micro, entrée MIDI ou monitoring local. Pas d’enregistrement direct dans l’onglet partagé ni de diffusion audio en direct entre ordinateurs.

**Toutes les applications doivent utiliser le nouveau protocole 12**, donc le même nouveau paquet.
Les versions précédentes en protocole 11 ou moins sont incompatibles avec ces sessions.
Les projets sauvegardés gardent leur format natif version 3 ; ce changement concerne la connexion.

## Ce qui a été vérifié pour ce palier

| Vérification | Résultat |
|---|---|
| Ensemble des tests sur ce Mac | **3 162 réussis, aucun échec**, dans 131 suites ; 59 contrôles non lancés dans le passage standard. Ce total comprend Suhina et nos ajouts. |
| Réseau et permissions | **83 tests réseau et 21 tests du cœur réussis** : invitations distinctes, remplacées ou déjà utilisées refusées, capacité limitée, séquences indépendantes et compteur sans fausse modification musicale. |
| Quatre participants | **Réussi avec de vrais échanges TLS entre quatre instances locales** : les changements convergent ; le départ d’un invité conserve les autres connexions ; son retour reçoit l’état actuel. |
| Conflits | **Réussi** : deux invités partant du même état ne s’écrasent pas ; le brouillon refusé peut être renvoyé avec la révision actualisée sans perdre sa séquence. |
| Audio et prises | **Réussi** : prises envoyées par trois auteurs, envois concurrents refusés puis repris, sons vérifiés reçus par tous, réutilisation d’un son sans doubler le budget et mêmes rendus. Les sources et sauvegardes sont conservées. |
| Lecteurs et copies | **Réussi** : quatre lecteurs à des positions différentes ; arrêt indépendant ; copies conservées après un départ, un retour et l’arrêt de l’hôte. |
| Interface | **16 tests multijoueur réussis**, dont le bouton d’invitation à capacité pleine, le compteur, la place libérée et la conservation des anciens onglets. |
| Rendu visuel | **Réussi séparément** : capture GPU avec trois invités admis vérifiée. |
| Qualité du code et documentation | **Réussi** : contrôle Rust strict, format, sources, documentation et six tests Python de distribution ; aucune nouvelle dépendance. |
| Paquet Mac Apple Silicon `4be2d443` | **Compilé et vérifié après extraction** : signature locale, trois exécutables, version de l’outil, sources exactes et sommes de contrôle. |
| Contrôles GitHub Mac/Windows/Linux | **Non lancés : GitHub refuse le déclenchement. Aucun résultat distant annoncé pour cette révision.** |
| Ordinateurs physiques | **Pas encore testé**, comme convenu. |

## Où trouver la version actuelle

- **Paquet local :** dossier `builds` voisin du dépôt, `suhina-multiplayer-guests-macos-arm64-4be2d443.zip`.
- **Dans le paquet :** `Suhina.app`, les trois exécutables nécessaires, ce suivi, les crédits et les sources exactes du commit livré.
- **Somme de contrôle :** fichier `.zip.sha256` à côté du ZIP ; les fichiers extraits sont vérifiés dans `SHA256SUMS.txt`.
- **Preuves locales :** dossier `validation/2026-10-28` voisin du dépôt : journaux, captures et script de fabrication du paquet.

Les tests et le paquet correspondent au code **`4be2d4439930db5f96aaea56c80aebe964b7ab1d`**,
compilé après son enregistrement puis envoyé sur notre branche GitHub. C’est un paquet de
développement Mac Apple Silicon, signé localement. L’application installée ou ouverte n’a pas
été remplacée. Tous les participants doivent utiliser cette même nouvelle version.

## GitHub et distribution

Notre branche **`feature/multiplayer`** est affichée à l’accueil du
[dépôt GitHub](https://github.com/Nozomagarion/suhina-daw). `main` reste le miroir de Suhina.
Le README et ses barres présentent notre application : **16 étapes réalisées sur 18 listées
pour créer ensemble**, avec les preuves dans `fork-progress.json`. Ce n’est pas un pourcentage
du logiciel final ; le nouveau palier correspond aux invitations de plusieurs musiciens.

**Blocage externe à nouveau constaté le 28 octobre à 10 h 36 (heure de Paris)**, sur le code
`4be2d4439930db5f96aaea56c80aebe964b7ab1d` :
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

**Préparer un set DJ avec des playlists dans l’application. Statut : à commencer.**
Après les outils communs et les sessions LAN à quatre, on avance sur la suite de la chaîne musicale.
Critère : créer, nommer et retrouver ses playlists, y ranger et réordonner les morceaux de la
bibliothèque DJ, puis les retrouver au redémarrage sans déplacer ni supprimer les fichiers audio.

Le multijoueur reste un prototype LAN : Internet, reconnexion automatique, noms/rôles et reprise
de l’hôte restent prévus. La version et les mises à jour communes restent à publier. Les points
cue, grilles rythmiques, platines et export vers le matériel DJ viennent après la préparation
des playlists. L’essai des ordinateurs de la maison reste différé, comme demandé.

Ce fichier est le point de référence à chaque palier ou blocage. Les anciens fichiers
`AUDIT_…` restent historiques. Les résultats externes sont des relevés datés ; ce document
n’implique pas de surveillance ou de travail automatiques entre nos échanges.
