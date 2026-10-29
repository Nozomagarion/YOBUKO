# Où en est notre application ?

## Chantier du 29 octobre : forme d’onde DJ

Le panneau **Prepare** charge maintenant une forme d’onde du morceau en arrière-plan,
avec les repères cue et les positions de départ et de lecture. Un clic choisit le départ ;
**Play from position** reste nécessaire pour écouter. Le calcul et le dessin existants du
studio sont réutilisés ; l’ouverture du panneau ne joue rien et ne modifie pas le morceau.

**3 252 tests Rust réussis, aucun échec, 65 tests ignorés ; 272 tests Python réussis.**
Les essais automatiques couvrent la fermeture pendant le chargement, les fichiers déplacés/modifiés,
les impulsions séparées sur les deux canaux et le clic de sélection sans démarrage audio.
Le contrôle Rust strict passe également ; les quatre captures DJ sont générées et examinées.
Après la finition du positionnement, les 27 contrôles ciblés DJ passent aussi.
La fabrication et la vérification du nouveau paquet Mac restent à terminer.
Le paquet Mac disponible reste `52ec1270` tant qu’un nouveau paquet n’est pas vérifié.
La grille rythmique, les platines et les exports matériels restent les étapes suivantes.


**Bilan du 28 octobre 2026 : préécoute DJ et points cue nommés réalisés et testés.**
Dans **DJ → Prepare**, on peut écouter un morceau depuis une position choisie, arrêter l’écoute,
ajouter des repères nommés, les déplacer et les retrouver au redémarrage dans toutes les playlists.
**3 246 tests du logiciel passent, aucun échec ; les 272 tests des scripts passent également.**
Le contrôle Rust strict de tout le projet, le format et les contrôles de documentation passent.
**Le paquet Mac `52ec1270` est compilé et vérifié après extraction**, avec les trois exécutables,
les sources exactes et ce suivi. L’application installée ou ouverte n’a pas été remplacée.

Le studio reste basé sur Suhina `9717cc71` et l’analyse rbxport sur `b739d369`. Le multijoueur à
quatre est conservé : protocole 13, copie portable v3 et projets natifs v4. Seul le catalogue DJ
passe au format 3, avec sauvegarde exacte de l’ancien catalogue avant le premier changement.
Les grilles rythmiques, les platines et l’export matériel restent à développer. La forme d’onde
a été ajoutée au palier du 29 octobre ci-dessus.

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
| **Espace DJ** | Bibliothèque persistante, analyse rbxport, rendu du studio, playlists, préécoute, cues et forme d’onde. | Grilles rythmiques, platines et export vers le matériel DJ. |
| **Mises à jour** | Bouton dans les paramètres pour comparer les versions GitHub ; suivi des sources et vérification des paquets de notre application. | Intégration régulière des nouveautés, publication et réception automatique d’une version commune. |

## Nouveau : les changements officiels de Suhina

- Égaliseur à huit bandes et améliorations de DJ EQ/import dans le studio.
- Nouveaux panneaux d’instruments et d’effets, dont FM Synth et DJ EQ.
- Courbes d’automatisation Bézier, avec le format natif de projet v4.
- Correctifs de chargement des plugins, liaisons ARM du processus de plugins et dépendances d’interface actualisées.
- Notre partage conserve ses limites et ses fonctions : trois invités, notes/clips/scènes, prises audio vérifiées, appareils FM Synth/Utility et lecteurs indépendants.
- La connexion compare aussi la révision du moteur Suhina. Une version différente est refusée avant l’envoi du morceau et sans consommer l’invitation.

**Les PR non fusionnées Saturator #1040 et contrôles XY #1028 ne sont pas intégrées.**
Les nouveaux effets du studio ne deviennent pas automatiquement éditables dans l’onglet partagé.

**Compatibilité des projets :** les anciens fichiers restent lisibles dans le studio. Les
nouvelles sauvegardes utilisent v4, que les anciennes applications ne lisent pas. Garder les
anciens fichiers, ou utiliser un autre nom de sauvegarde, pour les rouvrir avec l’ancien paquet.
Le partage ne réécrit pas les fichiers d’origine. Tous les participants doivent utiliser la
nouvelle version ; les anciennes invitations ne fonctionnent pas avec elle.

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

**Toutes les applications doivent utiliser le nouveau protocole 13**, donc le même nouveau paquet.
Les versions précédentes en protocole 12 ou moins sont incompatibles avec ces sessions.
Les nouvelles sauvegardes utilisent le format natif v4 ; les anciens projets restent lisibles.
Le partage utilise une copie portable v3 et conserve exactement les courbes prises en charge.

## Nouveau palier DJ : écouter et poser ses repères

**Code, tests, captures et paquet Mac validés localement.**

Les **133 suites** comptent **3 246 réussites, 0 échec, 64 contrôles non lancés dans le passage
standard**. Le périmètre DJ comprend **16 tests du catalogue, 10 tests des commandes et 10 tests
normaux de l’interface**. Les nouveaux scénarios vérifient le signal réellement rendu, sa position,
l’arrêt pendant un chargement, le changement d’onglet/dossier et les erreurs audio. Ils couvrent
aussi les repères, leur sauvegarde, les anciens catalogues et les brouillons périmés.
Les **3 tests GPU DJ passent séparément** ; les captures de la bibliothèque, des playlists et
de l’éditeur de repères ont été vérifiées. L’écoute physique au casque n’est pas annoncée comme testée.

Dans la bibliothèque ou une playlist, **Prepare** ouvre l’éditeur du morceau. Choisir une position
puis **Play from position** lance la préécoute. **Stop audition**, la fermeture de l’éditeur et le
retour au Studio arrêtent cette écoute et empêchent un chargement tardif de démarrer ensuite.
La lecture utilise la sortie casque configurée dans Suhina, ou la sortie principale à défaut.
La préécoute conserve les réglages audio existants de Suhina et ne modifie pas le projet Studio.

**Cue name** et **Cue position seconds** permettent d’ajouter un point nommé. Un point existant
se sélectionne pour changer son nom ou sa position ; sa suppression est confirmée. **Play cue**
l’écoute directement depuis ce repère. **Use playback position** reprend la position mesurée par
le moteur. Les points sont liés au morceau, dans toutes ses playlists, et conservés au redémarrage.

Le catalogue DJ passe au **format 3**. Les formats 1 et 2 sont repris sans écriture à la lecture ;
le premier changement garde une sauvegarde exacte. Les listes, leur ordre et leurs identifiants
restent conservés. Les écritures périmées ou invalides sont refusées. Une réanalyse garde les
repères et refuse de raccourcir le morceau au-delà d’un repère existant.

Le README compte **6 étapes DJ réalisées sur 9 critères listés** ; ces comptes ne mesurent pas
un pourcentage du logiciel final.

Limites : 16 points par morceau, 8 000 au total, positions en secondes. Préécoute à −6 dB,
lecture locale et un morceau à la fois. Les formes d’onde, grilles, calage automatique, platines,
partage des collections DJ et export USB/LINK restent à développer. Les écoutes réelles au casque
et les essais sur deux ordinateurs physiques restent distincts des tests de rendu automatique.

## Palier précédent : préparer ses playlists

**Code, tests, captures et paquet Mac validés localement.**

Les 133 suites comptent **3 232 tests réussis, 0 échec, 63 contrôles non lancés dans le passage
standard**. Le périmètre DJ comprend **10 tests du catalogue, 5 tests des commandes et 7 tests
normaux de l’interface**. Les scénarios couvrent la reprise de l’ancien catalogue, le redémarrage,
la réanalyse, la pagination, les brouillons périmés, les sauvegardes refusées et deux écritures
concurrentes. Les tests de collaboration et de réseau sont conservés dans la suite complète.
Les **deux tests GPU DJ passent séparément** et leurs captures ont été vérifiées : bibliothèque vide
et playlist ordonnée avec les boutons d’ajout, de déplacement et de retrait visibles.

- Créer et renommer un set depuis l’espace **DJ**, puis le choisir dans **DJ collection**.
- Ajouter les morceaux avec **Add from library** ; les flèches choisissent l’ordre de passage.
- Retirer une entrée ou supprimer une playlist après confirmation. Les fichiers audio restent en place.
- Retrouver les listes au redémarrage, y compris après la réanalyse d’un morceau déjà rangé.
- Recharger avec **Refresh DJ library** les changements d’une autre instance. Une écriture périmée
  est refusée ; un renommage qui échoue conserve le texte saisi.
- Reprendre l’ancien catalogue : sa lecture ne l’écrit pas et le premier changement garde une
  sauvegarde exacte avant le passage au format 2. Un catalogue invalide n’est pas écrasé.

Limites : playlists **locales**, jusqu’à 128 listes, 2 000 morceaux par liste et 16 000 entrées
au total. Elles ne sont pas encore partagées dans le multijoueur. Le format du projet Studio et
le protocole réseau ne changent pas. Les points cue, grilles, platines et export matériel restent à développer.

## Vérifications du palier précédent : mise à jour Suhina

| Vérification | Résultat |
|---|---|
| Ensemble des tests du logiciel sur ce Mac | **3 216 réussis, aucun échec**, dans 133 suites ; 62 contrôles non lancés dans le passage standard. Ce total comprend Suhina et nos ajouts. |
| Réseau et collaboration | **86 tests réseau et 21 tests du cœur réussis** : quatre instances locales, prises vérifiées, lecteurs indépendants, retour d’un invité, refus des anciens formats et des révisions de moteur différentes. |
| Anciens projets et courbes | **Réussi** : lecture d’un projet v3 puis partage v4, courbes Bézier conservées, formes invalides ou non canoniques refusées, réglage manuel d’un appareil sans supprimer ses points. |
| Studio et appareils officiels | **Réussi** : tests natifs EQ, DJ EQ, registre des appareils, automatisation et format v4 dans la suite complète. |
| DJ | **3 tests du module réussis**, avec les tests de son interface et du rendu vers la bibliothèque conservés. Le code d’analyse rbxport n’a pas changé. |
| Interface multijoueur | **16 tests réussis**, dont les boutons d’invitation, les quatre participants, les copies conservées et les commandes musicales. |
| Rendu visuel | **3 tests GPU séparés réussis** : panneaux FM Synth/EQ/DJ EQ, session avec trois invités et bibliothèque DJ ; cinq captures vérifiées. |
| Qualité, sources et dépendances | **Réussi** : contrôle Rust strict, format, en-têtes, documentation, aide des réglages, sources épinglées et audit des dépendances ; aucune exception de sécurité/licence ajoutée. |
| Scripts de maintenance et distribution | **272 tests réussis avec Bash 5.2.37**. Le Bash 3.2 livré par macOS ne prend pas en charge la syntaxe de certains scripts GitHub hérités ; l’outil récent a été construit dans un dossier temporaire, sans changer le shell du Mac ni affaiblir leurs contrôles. |
| Paquet Mac Apple Silicon `17a67aca` | **Compilé et vérifié après extraction** : signature locale, trois exécutables, version de l’outil, sources exactes et sommes de contrôle. |
| Contrôles GitHub Mac/Windows/Linux | **Non lancés : GitHub refuse le déclenchement** pour le nouveau code. Aucun résultat distant annoncé pour cette révision. |
| Ordinateurs physiques | **Pas encore testé**, comme convenu. |

## Où trouver la version actuelle

- **Paquet local :** dossier `builds` voisin du dépôt, `suhina-multiplayer-cues-macos-arm64-52ec1270.zip`.
- **Dans le paquet :** `Suhina.app`, les trois exécutables nécessaires, ce suivi, les crédits et les sources exactes du commit livré.
- **Somme de contrôle :** fichier `.zip.sha256` à côté du ZIP ; les fichiers extraits sont vérifiés dans `SHA256SUMS.txt`.
- **Preuves locales :** dossier `validation/2026-10-28` voisin du dépôt : journaux, captures et script de fabrication du paquet.

Les tests et le paquet correspondent au code **`52ec12704052412deb6a939a62d7b51312422bd9`**,
compilé après son enregistrement puis envoyé sur notre branche GitHub. C’est un paquet de
développement Mac Apple Silicon, signé localement. L’application installée ou ouverte n’a pas
été remplacée. Tous les participants doivent utiliser cette même nouvelle version.

La fabrication a utilisé un cache Rust temporaire extérieur au dépôt, supprimé après les vérifications
du paquet. Les preuves et les captures
sont conservées dans `validation/2026-10-28`, avec le script `package-cues.py` et le relevé
`cues-validation.json`. La version de secours précédente est conservée.

## GitHub et distribution

Notre branche **`feature/multiplayer`** est affichée à l’accueil du
[dépôt GitHub](https://github.com/Nozomagarion/suhina-daw). `main` reste le miroir de Suhina.
Le README et ses barres présentent notre application : **16 étapes réalisées sur 18 listées
pour créer ensemble**, avec les preuves dans `fork-progress.json`. Ce n’est pas un pourcentage
du logiciel final ; le nouveau palier correspond aux invitations de plusieurs musiciens.

**Blocage externe à nouveau constaté le 28 octobre à 22 h 47 (heure de Paris)**, sur le code
`52ec12704052412deb6a939a62d7b51312422bd9` :
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

**Afficher la forme d’onde et préparer la grille rythmique. Statut : à commencer.**
La préécoute et les repères nommés sont réalisés et testés. Le prochain critère est de visualiser
le morceau sur une échelle de temps, s’y déplacer et préparer des repères de pulsation éditables,
sans modifier le fichier audio. Les platines et l’export matériel viennent ensuite.
Les playlists et les points cue restent locaux pour le moment.

Le multijoueur reste un prototype LAN : Internet, reconnexion automatique, noms/rôles et reprise
de l’hôte restent prévus. La version et les mises à jour communes restent à publier. Les grilles rythmiques, platines et export vers le matériel DJ restent à développer. L’essai des ordinateurs de la maison reste différé, comme demandé.

Ce fichier est le point de référence à chaque palier ou blocage. Les anciens fichiers
`AUDIT_…` restent historiques. Les résultats externes sont des relevés datés ; ce document
n’implique pas de surveillance ou de travail automatiques entre nos échanges.
