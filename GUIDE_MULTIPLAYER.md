# Premier essai du multijoueur Suhina

Cette version vise un réseau local : **un hôte et jusqu’à trois invités**. Chaque personne
écoute et lance la lecture sur son ordinateur. Les modifications prises en charge sont
confirmées par l’hôte ; l’audio n’est pas diffusé en direct sur le réseau.

## Préparer les ordinateurs

Installer **la même version et le même identifiant de build** sur tous les ordinateurs, avec
l’archive adaptée à chaque système : Mac et Windows ont des fichiers différents. Ne pas comparer
seulement le numéro générique «0.0.1». Une ancienne révision Suhina peut être refusée
même si son interface semble identique. Garder ses projets habituels et une copie de secours.
La nouvelle version lit les anciens projets ; ses nouvelles sauvegardes utilisent le format v4.

Brancher les ordinateurs au même réseau local et choisir la sortie audio dans les paramètres.
Si le système demande une autorisation réseau pour Suhina, autoriser le réseau privé utilisé.
Les réglages et les projets ne doivent pas être placés dans le dossier de l’application.

## Se connecter une première fois

1. Sur l’ordinateur hôte, ouvrir Suhina puis **Multiplayer** en haut de la fenêtre. Pour ce
   premier essai, aucun projet personnel n’est nécessaire.
2. Dans **Host address**, utiliser l’adresse IPv4 privée de cet ordinateur, suivie de `:0`
   pour choisir un port libre. L’adresse proposée est un point de départ. `127.0.0.1` ne permet
   de joindre que ce même ordinateur ; pour la maison, utiliser son adresse `192.168…`, `10…`
   ou l’autre adresse privée indiquée par les paramètres réseau du système.
3. Cliquer **Create LAN session** : une copie de démonstration à quatre pistes s’ouvre dans
   un nouvel onglet partagé. Les éventuels projets déjà ouverts sont conservés.
4. Cliquer **Copy invitation** et transmettre le texte en privé à une personne. L’invitation
   expire après cinq minutes et ne sert qu’une fois. Ne pas créer la suivante avant que la
   personne ait rejoint, car elle remplacerait l’invitation encore inutilisée.
5. Sur l’autre ordinateur, ouvrir **Multiplayer**, coller cette invitation dans le champ prévu
   puis cliquer **Join LAN session**. Attendre l’ouverture de l’onglet partagé. Le nombre
   **Guests: N / 3** permet de vérifier la présence de l’invité.
6. Pour une autre personne, l’hôte clique **New invitation**, puis répète les étapes 4–5.
   Trois invités au maximum peuvent être présents en même temps.

## Vérifier que l’on travaille ensemble

Sur un ordinateur, renommer une piste avec **Rename**, puis constater le même nom sur l’autre.
Tester **Set volume** et **Set pan**. Dans **MIDI notes**, choisir un clip et modifier une note
avec **Apply note** ; l’hôte confirme la modification. Chaque personne peut cliquer **Play locally**
ou **Stop locally** sans déplacer la lecture de l’autre.

Si deux modifications partent depuis le même état, l’une peut être refusée : lire le message,
rafraîchir le brouillon puis refaire l’action souhaitée. Aucun refus ne doit disparaître avant
de pouvoir être lu ; **Dismiss multiplayer message** permet de l’acquitter. Un refus ne signifie
pas que le projet a été modifié. Attendre la confirmation avant d’enchaîner les changements.

## Partager son morceau et ses prises

Après la démonstration, utiliser **Share current MIDI project** ou **Share project + audio**
depuis un projet Studio ouvert. Cela partage une copie et conserve l’original. Les instruments
et effets partagés pris en charge sont **FM Synth et Utility** ; les plugins externes et les
autres appareils ne sont pas encore partagés. Jusqu’à 64 pistes sont admises par le format borné.

L’option audio transfère les sons déjà chargés dans le Studio, jusqu’à 64 sons, 64 Mio par son
et 256 Mio au total. Attendre le transfert vérifié avant l’ouverture de la copie. Les copies audio
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
