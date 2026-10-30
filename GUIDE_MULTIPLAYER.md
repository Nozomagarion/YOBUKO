# Premier essai du multijoueur Suhina

Cette version fonctionne **par Internet, avec un simple code** : l’hôte crée une session, envoie
le code (sur Discord ou n’importe quelle messagerie) et l’autre personne le colle pour rejoindre,
même à des centaines de kilomètres. Aucun compte, aucun réglage de box. **Un hôte et jusqu’à trois
invités.** Chaque personne écoute et lance la lecture sur son ordinateur. Les modifications prises
en charge sont confirmées par l’hôte ; l’audio n’est pas diffusé en direct. Le mode réseau local
reste disponible.

## Préparer les ordinateurs

Installer **la même version et le même identifiant de build** sur tous les ordinateurs, avec
l’archive adaptée à chaque système : Mac et Windows ont des fichiers différents. Ne pas comparer
seulement le numéro générique «0.0.1». Une ancienne révision Suhina peut être refusée
même si son interface semble identique. Garder ses projets habituels et une copie de secours.
La nouvelle version lit les anciens projets ; ses nouvelles sauvegardes utilisent le format v4.

Chaque ordinateur a simplement besoin d’Internet. Choisir la sortie audio dans les paramètres.
Si le système demande une autorisation réseau pour Suhina, l’accepter.
Les réglages et les projets ne doivent pas être placés dans le dossier de l’application.

## Se connecter une première fois

1. Sur l’ordinateur hôte, ouvrir Suhina puis **Multiplayer** en haut de la fenêtre. Laisser
   **Over the Internet** coché. Pour ce premier essai, aucun projet personnel n’est nécessaire.
2. Cliquer **Create session**. Suhina se connecte à Internet (quelques secondes) puis une copie
   de démonstration à quatre pistes s’ouvre dans un nouvel onglet partagé. Les éventuels projets
   déjà ouverts sont conservés.
3. Cliquer **Copy invitation code** et envoyer le code en privé à une personne, par exemple sur
   Discord. Le code expire après cinq minutes et ne sert qu’une fois. Ne pas créer le suivant
   avant que la personne ait rejoint, car il remplacerait le code encore inutilisé.
4. Sur l’autre ordinateur, ouvrir **Multiplayer**, coller le code dans **Invitation code from the
   host** puis cliquer **Join session**. Attendre l’ouverture de l’onglet partagé (jusqu’à une
   vingtaine de secondes la première fois). Le nombre **Guests: N / 3** confirme la présence de
   l’invité.
5. Pour une autre personne, l’hôte clique **New invitation**, puis répète les étapes 3–4.
   Trois invités au maximum peuvent être présents en même temps.

Les ordinateurs se parlent directement à travers les box. Quand une box bloque, un relais public
gratuit transmet les données chiffrées sans pouvoir les lire. Pour un réseau local sans Internet,
cocher **On this local network** et saisir dans **Host address** l’adresse privée de l’hôte suivie
de `:0`.

## Vérifier que l’on travaille ensemble

Sur un ordinateur, renommer une piste avec **Rename**, puis constater le même nom sur l’autre.
Tester **Set volume** et **Set pan**. Dans **MIDI notes**, choisir un clip et modifier une note
avec **Apply note** ; l’hôte confirme la modification. Chaque personne peut cliquer **Play locally**
ou **Stop locally** sans déplacer la lecture de l’autre.

Pour écouter la même chose au même moment, cocher **Link playback** (n’importe quel participant
peut le faire). Les boutons deviennent **Play together**, **Stop together** et **Everyone from
beat 16** : chaque clic lance, arrête ou déplace la lecture sur tous les ordinateurs à la fois.
Le départ est prévu environ 0,3 seconde plus tard pour que tout le monde démarre ensemble ;
l’audio n’est toujours pas diffusé, chaque ordinateur joue sa propre copie. Garder l’onglet
partagé au premier plan. Décocher la case rend la lecture de nouveau indépendante.

Si deux modifications partent depuis le même état, l’une peut être refusée : lire le message,
rafraîchir le brouillon puis refaire l’action souhaitée. Aucun refus ne doit disparaître avant
de pouvoir être lu ; **Dismiss multiplayer message** permet de l’acquitter. Un refus ne signifie
pas que le projet a été modifié. Attendre la confirmation avant d’enchaîner les changements.

## Partager son morceau et ses prises

Après la démonstration, utiliser **Share current MIDI project** ou **Share project + audio**
depuis un projet Studio ouvert. Cela partage une copie et conserve l’original. Les instruments
et effets partagés pris en charge sont **FM Synth et Utility** ; les plugins externes et les
autres appareils ne sont pas encore partagés. Jusqu’à 64 pistes sont admises par le format borné.

L’option audio transfère les sons déjà chargés dans le Studio, jusqu’à 4096 sons, 2 Gio par son
et 16 Gio au total. Les projets lourds sont pris en charge : les sons voyagent par morceaux
vérifiés un à un, sans être chargés en entier en mémoire pendant le transfert. Chaque ordinateur
garde les sons reçus dans le dossier **Sound cache** (jusqu’à 20 Gio, les plus anciens sont
effacés d’abord) : un son déjà reçu n’est jamais renvoyé, même lors d’une session suivante, et un
transfert coupé reprend là où il s’était arrêté. Quand il manque plus de 32 Mio de sons, la copie
s’ouvre tout de suite et les sons arrivent ensuite ; leurs clips restent muets jusque-là. Les copies audio
sont conservées dans le sous-dossier **Shared projects** du dossier de données de l’application.

Pour une nouvelle prise, enregistrer et terminer dans un onglet Studio local, puis ouvrir
**Publish take**, choisir le son chargé, la piste et la position, et utiliser **Publish loaded take**.
Les invités peuvent également publier. Le son entier est envoyé ; les découpes et effets locaux
ne sont pas automatiquement intégrés. L’enregistrement direct dans l’onglet partagé n’est pas fourni.

## Quitter et reprendre

**Leave session** conserve la dernière copie reçue et la rend modifiable localement. Le départ
d’un invité laisse les autres connectés. Pour revenir, demander une nouvelle invitation : une
nouvelle copie s’ouvre dans un autre onglet, sans fusionner les anciennes modifications locales.
Le départ de l’hôte ferme les connexions ; les copies des participants restent conservées.
Enregistrer normalement les copies que l’on souhaite garder.

## Versions et aide

**Settings → Agents and updates → GitHub updates…** compare les sources et recherche un paquet
combiné. Une nouvelle révision officielle peut nécessiter une intégration : ce bouton ne remplace
pas notre application par Suhina officiel. Tous les participants doivent passer au même nouveau
paquet. Le canal public commun et ses plateformes validées sont indiqués dans le suivi.

L’état exact de livraison, les contrôles et les essais physiques encore à faire sont dans
[SUIVI_PROJET.md](SUIVI_PROJET.md). Ce guide ne prétend pas que l’essai sur les deux ordinateurs
de la maison a déjà été effectué. Internet, reprise automatique après coupure et transfert du
rôle d’hôte restent des extensions ultérieures. La bascule vers rbxport est un chantier distinct ;
aucune nouvelle fonction de platines, vitesse ou boucles n’est développée pour cette version.
