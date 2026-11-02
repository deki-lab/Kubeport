# UX spec : panneau de la barre de menu

Statut : proposition, à valider par le propriétaire. Aucun code modifié.
Auteur : Uxie. Base : le code de `app/Sources/Kubeport/` à la version 0.1.0. **Personne n'a vu l'interface à l'écran** : tout ce qui touche au rendu (hauteurs, contrastes, comportement du `ScrollView` dans `MenuBarExtra`) est une estimation, marquée « à vérifier au rendu ».

Documents lus : `GLOSSARY.md`, ADR 0002 et 0003, README (Utilisation), les cinq fichiers Swift du panneau, `ffi/src/lib.rs`. `PRODUCT.md`, `DESIGN.md`, `CLAUDE.md` et `AGENTS.md` n'existent pas dans le dépôt : la spec s'appuie sur le glossaire, les ADR et l'interface existante. Pas de section « Development cycle » : les commandes de test citées en DoD sont celles du README (`scripts/measure-footprint.sh`).

## Contexte

Demande du propriétaire : « pour le design, tu peux me faire une proposition », avec une question précise : **le panneau défile-t-il avec une vingtaine de Forwards ?**

Réponse : oui, et aujourd'hui mal. Avec le code actuel, une ligne fait environ 36 pt (nom + sous-titre + espacement), un titre de groupe environ 21 pt, et la zone de liste plafonne à 420 pt. Vingt Forwards en trois groupes demandent environ 800 pt : on voit une dizaine de lignes, il faut défiler pour le reste, et la barre de défilement macOS est masquée par défaut (indicateur « overlay »). Un Forward en Erreur à la quinzième ligne est invisible : seul le point rouge de l'icône le signale, sans dire lequel.

Décisions déjà prises, non rouvertes : une paire de ports par Forward ; 127.0.0.1 uniquement ; activer un Forward qui partage un port local désactive l'autre (Forwards concurrents) avec un bref avis ; look natif ; mémoire minimale ; panneau fluide et instantané. Contraintes ADR 0003 : `MenuBarExtra(.window)`, pas de webview, ≤ 35 Mo au repos avec 5 Forwards Actifs (17 Mo aujourd'hui).

Vocabulaire : celui de `GLOSSARY.md` (Forward, Cible, Contexte, Pod résolu, État voulu Actif/Inactif, État réel Connecté/Reconnexion/Erreur, Forwards concurrents). Les mots proscrits par le glossaire (tunnel, statut, échec, conflit de port, doublon, pod actif, on/off, en ligne) n'apparaissent dans aucun texte d'interface. « Namespace » reste tel quel (terme Kubernetes absent du glossaire). Tutoiement, comme le texte actuel (« Crée-en un »).

---

## 1. Critique du panneau actuel

Constats tirés du code, du plus grave au plus léger.

### Bloquants (l'utilisateur ne peut pas finir ou ne comprend pas)

1. **Faux état vide au lancement.** `ListScreen` affiche « Aucun Forward. Crée-en un… » dès que `model.forwards` est vide. Or le cœur n'envoie la première liste qu'après la lecture du shell de login (jusqu'à 5 s, `ffi/src/lib.rs`, `run()`) et la réactivation des Forwards Actifs. Pendant ce temps un utilisateur qui a 20 Forwards en voit zéro, et « Nouveau Forward » est actif alors que la boucle du cœur ne répond pas encore : le formulaire reste vide.
2. **Erreur de démarrage : boutons morts.** `startError` s'affiche (config TOML invalide, cas réaliste puisque le fichier s'édite à la main), mais « Ouvrir la config » et « Recharger » ne font rien : `configPath` vient de `core?.configPath()` et `core` est `nil`. C'est exactement le moment où l'utilisateur a besoin d'ouvrir le fichier. Aucun moyen de réessayer sans quitter l'app.
3. **Le switch n'a pas de nom pour VoiceOver.** `Toggle("", …).labelsHidden()` : VoiceOver annonce « bouton à bascule, activé » sans dire de quel Forward il s'agit. Le point de couleur n'est ni masqué ni décrit. Un utilisateur VoiceOver ne peut pas utiliser le panneau.
4. **Modifier et Dupliquer : clic droit seulement.** Seule trace : une infobulle, qui apparaît après un délai et n'est pas lue par VoiceOver. Pas de chemin clavier. Le README est la seule documentation de la fonction.
5. **L'avis disparaît si on change d'écran.** `notice` n'est rendu que par `ListScreen`. L'avis de `setLaunchAtLogin` (émis depuis Réglages) et l'avis « config non rechargée » ne sont jamais vus s'ils arrivent pendant que le formulaire ou les Réglages sont ouverts. Il disparaît seul après 6 s, sans bouton, et un second avis écrase le premier.
6. **Suppression : double-clic = suppression.** Le premier clic change le libellé du même bouton, à la même place, en « Confirmer la suppression ». Un double-clic supprime. Rien ne réinitialise la confirmation.

### À corriger avant la sortie (gêne réelle)

7. **Statut porté par la couleur seule pour Connecté.** Connecté = point vert, Inactif = point gris, sans texte ni forme différente. Reconnexion et Erreur ont un texte dans le sous-titre, pas Connecté. Le vert `.green` sur fond clair a un contraste faible (à vérifier au rendu).
8. **Densité et lisibilité à 20 Forwards.** Toutes les lignes ont deux lignes de texte, y compris les Forwards Inactifs et Connectés dont le sous-titre répète `localhost:port`. Aucune recherche, aucun repli des groupes, titres de groupe non épinglés : en bas de liste on ne sait plus dans quel Contexte on est. Un nom de Contexte EKS (`arn:aws:eks:…`) écrase le titre de groupe (`"\(context) · \(namespace)"`, une seule ligne, sans règle de troncature).
9. **Switch `.mini`.** Cible d'environ 26 × 15 pt, sur 20 lignes serrées : clics ratés, donc activation du mauvais Forward.
10. **Un Forward en Erreur n'a pas de suite.** La cause demande « une action humaine » (glossaire), mais la ligne n'offre ni « Réessayer » ni « Modifier » : il faut décocher puis recocher le switch, ou faire un clic droit non découvrable. La raison est tronquée à 2 lignes sans moyen de la lire en entier ni de la copier.
11. **Formulaire : listes en chargement sans indication.** `namespaces()` et `targets()` font des appels réseau (auth `exec` possible, donc plusieurs secondes) sans délai maximal côté cœur. Pendant l'attente, le `Picker` est vide ou garde les valeurs du namespace précédent (réponses non ordonnées : une réponse tardive peut écraser la bonne). Pas de spinner, pas de délai d'abandon.
12. **Formulaire : Contexte injoignable.** Le message `Namespaces non listés (…) : saisie libre.` apparaît en bas, en gris secondaire, loin du champ concerné, sans bouton « Réessayer ». Le texte est une phrase technique, pas une consigne.
13. **Formulaire : cas vides sans message.** Aucun Contexte dans le kubeconfig : `Picker` vide, Enregistrer grisé, aucune explication. Aucun Service dans le namespace : le champ devient un `TextField` libre, sans dire pourquoi. En modification, si le Contexte du Forward a disparu du kubeconfig (cas « Contexte absent »), le `Picker` n'a pas de sélection valide.
14. **Formulaire : validation tardive.** Nom déjà utilisé : erreur seulement après Enregistrer (le cœur refuse `DuplicateName`). Port 0 accepté par `UInt16`. Rien n'avertit qu'un autre Forward partage déjà le port local, ni que modifier un Forward Actif le relance (comportement documenté seulement dans le README).
15. **Pied de panneau chargé.** Six commandes de même poids visuel sur deux lignes, boutons `.borderless` sans apparence de bouton. « Tout désactiver » (sans annulation possible, et qui réécrit `state.toml`) est collé à « Nouveau Forward ». « Recharger » n'a aucun retour quand tout va bien.
16. **Réglages : « Enregistrer » ne porte que sur le kubeconfig.** Le switch de lancement à l'ouverture de session s'applique tout de suite ; l'utilisateur croit devoir enregistrer pour les deux. L'indication « Réglages Système > Général > Ouverture » est un chemin que macOS renomme d'une version à l'autre.
17. **Icône de la barre de menu.** (a) L'état Erreur est un bitmap non template dessiné avec `NSColor.labelColor` : cette couleur suit l'apparence de l'app, pas celle de la barre de menu, qui dépend du fond d'écran. Risque d'icône sombre sur barre sombre (à vérifier au rendu, clair et sombre, fonds variés). (b) Le point rouge est le seul signal, sans forme distincte. (c) `accessibilityDescription` = « Kubeport : erreur », sans nombre.
18. **État de l'écran à la réouverture.** `@State screen` dans `PanelView` : en `.window`, la vue peut survivre à la fermeture du panneau, qui rouvre alors sur un formulaire à moitié rempli ou sur la position de défilement précédente (à vérifier au rendu).
19. **Hauteur instable.** Le panneau prend la hauteur de son contenu : elle saute entre liste, formulaire et réglages, et à chaque ajout ou retrait de ligne, ce qui déplace les boutons sous la souris.
20. **Raisons en `Reconnexion` à la première connexion.** Activer un Forward donne d'abord « Reconnexion : recherche d'un pod Ready » (`forward.rs:108`). Pour l'utilisateur c'est une première connexion, pas une reconnexion. Voir question ouverte 1.

### Ce qui marche et reste

Panneau de 360 pt ; regroupement par Contexte et namespace ; switch comme geste principal ; formulaire en cascade avec listes remplies depuis le cluster et nom auto `cible:port` ; icône à trois états ; notification uniquement au passage en Erreur ; Échap annule le formulaire ; Entrée l'enregistre ; avis « Forward désactivé : port local partagé ».

---

## 2. Proposition

Principe directeur : **le panneau répond à une seule question, « est-ce que mes Forwards tournent ? », en un coup d'œil, puis laisse agir sur un Forward en un clic.** Tout le reste (édition, réglages, config) est à un niveau de plus. Rien ne s'ajoute pour 5 Forwards : les aides à l'échelle (recherche, filtre, repli) n'apparaissent qu'à partir de 10 Forwards.

### 2.1 Structure du panneau

De haut en bas, largeur 360 pt inchangée :

1. **En-tête** (36 pt) : à gauche le **résumé d'état** (texte, voir 2.3), à droite le bouton **+** (Nouveau Forward).
2. **Recherche et filtre** (30 pt), **seulement au-delà de 10 Forwards**.
3. **Liste** : défilante, titres de groupe épinglés.
4. **Avis** (apparaît au besoin, au-dessus du pied).
5. **Pied** (34 pt) : « Tout désactiver » à gauche, menu **⋯** à droite.

Le bouton « Nouveau Forward » passe en tête, toujours visible quel que soit le défilement. Le menu ⋯ regroupe ce qui est rare : Réglages…, Ouvrir la config, Recharger la config, Quitter Kubeport. L'engrenage et le bouton power disparaissent du pied.

### 2.2 Comportement avec 1, 5 et 20+ Forwards

| | 1 à 4 Forwards | 5 à 10 | 11 à 20+ |
|---|---|---|---|
| En-tête avec résumé | oui | oui | oui |
| Titres de groupe | oui, non repliables en pratique | oui, repliables | oui, repliables, **épinglés** |
| Recherche + filtre | non | non | **oui** |
| Hauteur de la liste | celle du contenu | celle du contenu (≤ 440 pt) | **fixe 440 pt** (stable) |
| Défilement | non | rarement | oui, indicateur toujours visible |

Règles :

- **Hauteur de liste** : `min(contenu, 440 pt)` jusqu'à 10 Forwards ; **fixe à 440 pt** au-delà, pour que le pied ne bouge pas quand on filtre ou replie. Plafond de sécurité : `visibleFrame` de l'écran moins 200 pt (petit écran ou barre de menu double). Environ 14 lignes de 28 pt visibles en même temps contre une dizaine aujourd'hui. À vérifier au rendu : que le `ScrollView` prenne bien la hauteur du contenu sous ce plafond dans `MenuBarExtra`.
- **Défilement** : `LazyVStack(pinnedViews: [.sectionHeaders])` dans un `ScrollView` avec `.scrollIndicators(.visible)` : la barre est visible même sans trackpad, ce qui répond à « y a-t-il autre chose en dessous ? ». Les titres de groupe restent épinglés en haut avec un fond opaque (matériau de fenêtre) : on sait toujours dans quel Contexte on se trouve.
- **Groupes repliables** : un clic sur le titre (ou ← / → au clavier) replie ou déplie. L'état est mémorisé (`UserDefaults`, liste des groupes repliés) et ne change jamais tout seul. Un groupe replié affiche son **résumé** : nombre de Forwards, nombre d'Actifs, et un `▲ N en Erreur` ou `↻ N en Reconnexion` s'il en cache. **Un Erreur ne peut donc jamais être caché par un repli.**
- **Recherche** : filtre en direct sur nom, Contexte, namespace, nom de Cible et port local. Un groupe sans correspondance disparaît ; les groupes repliés qui contiennent une correspondance s'affichent dépliés le temps de la recherche, sans écraser le choix mémorisé.
- **Filtre d'état** (menu à droite du champ, glyphe `line.3.horizontal.decrease.circle`) : « Tous les Forwards » (défaut), « Actifs seulement », « En Reconnexion ou en Erreur ». C'est notre réponse à « épingler les Actifs » : un filtre, pas un réordonnancement, donc aucune ligne ne saute sous la souris. Le filtre actif se voit : le glyphe devient plein et le champ affiche une puce « Actifs ✕ ». Il est remis à « Tous » à chaque ouverture du panneau pour qu'un Forward ne paraisse jamais « disparu ».
- **Ordre** : celui du cœur (ordre de la config) dans le groupe ; groupes triés par Contexte puis namespace, comme aujourd'hui. Pas de tri par état, qui ferait sauter les lignes.
- **« Épingler » des Forwards favoris** : rejeté pour la v0.2. Il demande un champ dans `config.toml` (changement du cœur et du format) pour un gain que repli + recherche + filtre couvrent.

### 2.3 Résumé d'état (en-tête)

Texte, pas seulement des pastilles : « 4 Connectés · 1 en Reconnexion · 1 en Erreur ». Un compteur à zéro est omis. Chaque élément porte son glyphe (voir 2.4) et la couleur du glyphe. Cas : aucun Actif → « Aucun Forward Actif » ; liste vide → « Kubeport ». Cette ligne remplace la détection à l'œil par le point rouge et sert aussi de libellé VoiceOver global.

### 2.4 Anatomie d'une ligne de Forward

Ligne repos (hauteur 28 pt, une seule ligne de texte) :

```
 [glyphe]  nom du Forward ........... :port local   [switch]
```

- **Glyphe d'état** (14 pt, forme distincte, pas la couleur seule) :

  | État | Glyphe SF Symbols | Couleur | Texte VoiceOver |
  |---|---|---|---|
  | Connecté | `circle.fill` | vert système | « Connecté » |
  | Reconnexion | `arrow.triangle.2.circlepath` | orange système | « Reconnexion » |
  | Erreur | `exclamationmark.triangle.fill` | rouge système | « Erreur » |
  | Inactif | `circle` (contour) | gris secondaire | « Inactif » |

  Pas d'animation de rotation : le glyphe est statique, donc aucune charge CPU au repos.
- **Nom** : corps de texte standard, une ligne, troncature en fin avec infobulle du nom complet. En gris secondaire pour un Forward Inactif.
- **Port local** : `:8080`, police chiffres à chasse fixe, gris secondaire, aligné à droite. `localhost:` n'est pas répété à chaque ligne : il figure dans le détail.
- **Switch** : taille `.small` (au lieu de `.mini`), cible ≥ 32 × 18 pt, étiquette accessible « Activer le Forward {nom} », valeur « Actif » ou « Inactif ».
- **Sous-ligne**, seulement pour Reconnexion et Erreur : la raison renvoyée par le cœur, en couleur d'état, 2 lignes maximum. Aucune sous-ligne pour Connecté et Inactif.
- **Ligne d'actions d'une ligne en Erreur** : deux boutons de texte, « Réessayer » (désactive puis réactive, côté Swift) et « Modifier… ».

**Détail d'une ligne (clic sur la ligne, hors switch)** : un seul détail ouvert à la fois, la ligne se déploie en place :

- `Cible` : `svc/api:80` ou `pod/api-0:80` ;
- `Pod résolu` : seulement quand le Forward est Connecté ;
- `Adresse` : `localhost:8080` avec bouton « Copier » ;
- si un autre Forward partage le port local : une phrase « Forward concurrent : … » (voir copie) ;
- boutons « Modifier… » et « Dupliquer… ».

Le clic droit existant est conservé comme raccourci, avec les mêmes entrées. Le survol met la ligne en surbrillance (affordance du clic) ; l'infobulle actuelle disparaît, son contenu étant dans le détail.

### 2.5 Écrans

**Formulaire.** Même cascade (Contexte → namespace → Type → Cible → port distant → port local → Nom), mais :

- en-tête avec **« ‹ Forwards »** (retour, équivaut à Annuler) et le titre à droite ;
- **chaque liste dépendante a son état de chargement dans le champ** (spinner + « Chargement des namespaces… »), le champ est désactivé pendant ce temps, et les réponses périmées sont ignorées (jeton de requête par chargement) ;
- **délai d'abandon de 10 s** pour namespaces et Cibles : au-delà, message inline sous le champ concerné + bouton « Réessayer », et le champ passe en saisie libre (comportement actuel, mais expliqué) ;
- **validation inline** et immédiate : nom déjà utilisé, port hors 1–65535 ; le bouton Enregistrer grisé porte une infobulle qui dit ce qui manque ;
- **information** (pas une erreur) quand le port local est déjà celui d'un autre Forward : Forwards concurrents ;
- en modification d'un Forward Actif : « Ce Forward est Actif : il sera relancé à l'enregistrement. » ;
- **suppression** : le bouton « Supprimer » ouvre une ligne de confirmation distincte (« Supprimer « nom » ? »), avec « Annuler » (focus par défaut) et « Supprimer » ; la confirmation se referme après 5 s. Si le Forward est Actif : « Il sera d'abord arrêté. » ;
- si le Contexte du Forward n'est plus dans le kubeconfig, il reste sélectionné dans la liste avec le suffixe « (absent du kubeconfig) » et un message inline.

**Réglages.** En-tête « ‹ Forwards » + « Réglages ». Le switch de lancement s'applique **immédiatement** (pas besoin d'enregistrer). Le Kubeconfig a son champ, un bouton « Choisir… » (`NSOpenPanel`) et son propre bouton « Appliquer ». Plus de bouton « Fermer » : le retour est dans l'en-tête, Échap aussi. Pied : version et lien « Ouvrir la config ».

**Avis.** Barre au-dessus du pied, **affichée par `PanelView`** (donc sur tous les écrans), icône `info.circle`, texte sur 3 lignes maximum, bouton ✕. Durée : `max(6 s, 0,06 s × nombre de caractères)`, plafonnée à 20 s. Chaque nouvel avis est annoncé à VoiceOver (`AccessibilityNotification.Announcement`).

**Icône de la barre de menu.** Trois états conservés (contour / pleine / pleine avec point rouge), avec :
- `accessibilityDescription` dynamique : « Kubeport : aucun Forward Actif », « Kubeport : 5 Forwards Actifs », « Kubeport : 5 Forwards Actifs, 2 en Erreur » ;
- état Erreur à valider au rendu sur barre claire, sombre et fonds d'écran variés (point 17 de la critique). Si l'icône est illisible, repli : symbole template avec badge d'exclamation découpé (lisible partout) plutôt qu'un bitmap coloré ;
- pas d'état distinct pour Reconnexion : elle est fréquente (réveil, changement de réseau) et rendrait l'icône bavarde.

### 2.6 Clavier

macOS ne donne pas le focus clavier aux boutons et switches par Tab sauf si « Accès au clavier complet » est activé (réglage système, désactivé par défaut). Le panneau fournit donc sa propre navigation :

| Touche | Action |
|---|---|
| ↑ / ↓ | Déplacer le focus entre titres de groupe et lignes |
| Espace | Activer ou désactiver le Forward ciblé |
| Entrée | Ouvrir ou fermer le détail de la ligne ciblée |
| ← / → sur un titre de groupe | Replier / déplier |
| ⌘F | Aller au champ de recherche |
| ⌘N | Nouveau Forward |
| ⌘, | Réglages |
| ⌘R | Recharger la config |
| ⌘Q | Quitter |
| Échap | Ferme le détail ouvert, puis efface la recherche, puis ferme le panneau |

À l'ouverture : focus dans le champ de recherche si plus de 10 Forwards (saisie immédiate), sur la première ligne sinon. Dans les écrans formulaire et réglages, Tab suit l'ordre visuel de haut en bas.

### 2.7 Mémoire et ouverture instantanée

- Aucune donnée dérivée (groupes, compteurs, filtre) calculée dans `body` : `CoreModel` la recalcule à chaque `forwardsChanged` (une fois par événement, pas par rendu).
- `LazyVStack` : seules les lignes visibles sont instanciées, que la liste en ait 20 ou 200.
- Pas de timer, pas d'animation au repos, pas d'image bitmap : glyphes SF Symbols uniquement.
- Seul état persistant ajouté : un tableau de noms de groupes repliés dans `UserDefaults`.
- Aucun appel réseau ni attente dans le chemin d'ouverture. Les listes du formulaire se chargent après l'affichage du formulaire.
- Le filtre, la recherche et le détail ouvert sont de l'état de vue (`@State`), remis à zéro à chaque ouverture.

---

## 3. Maquettes

Légende : `●` Connecté, `↻` Reconnexion, `▲` Erreur, `○` Inactif ; switch `[ ●]` Actif, `[● ]` Inactif ; `┃` curseur de la barre de défilement. Les maquettes sont à l'échelle de la largeur, pas de la hauteur.

### 3.1 Liste, 20 Forwards en 3 groupes (tout déplié, défilé tout en haut)

Groupes : `eks-prod · payments` (8), `eks-staging · api` (7), `docker-desktop · postiz` (5). Visible ici : 12 Forwards sur 20 ; la barre de défilement indique le reste. Les titres restent épinglés en haut pendant le défilement.

```
┌──────────────────────────────────────────────────────┐
│ 4 Connectés · 1 en Reconnexion · 1 en Erreur     [+] │
│ ⌕ Filtrer…                                     [≡ ▾] │
├──────────────────────────────────────────────────────┤
│ ▾ eks-prod · payments                  8 · 3 actifs ┃│
│ ● ledger-db:5432                        :5432  [ ●] ┃│
│ ● ledger-ro:5433                        :5433  [ ●] ┃│
│ ○ redis-cache:6379                      :6379  [● ] ┃│
│ ○ kafka-ui:8080                         :8080  [● ] ┃│
│ ↻ grafana:3000                          :3000  [ ●] ┃│
│   recherche d'un pod Ready                          ┃│
│ ○ prometheus:9090                       :9090  [● ] ││
│ ○ vault:8200                            :8200  [● ] ││
│ ○ jaeger:16686                         :16686  [● ] ││
│ ▾ eks-staging · api                    7 · 2 actifs ││
│ ● api:8080                              :8080  [ ●] ││
│ ○ api-debug:5005                        :5005  [● ] ││
│ ▲ pg-staging:5432                       :5432  [ ●] ││
│   authentification refusée : token expiré           ││
│   [Réessayer] [Modifier…]                           ││
│ ○ pg-replica:5433                       :5433  [● ] ││
├──────────────────────────────────────────────────────┤
│ Tout désactiver                                  [⋯] │
└──────────────────────────────────────────────────────┘
```

### 3.2 Même liste, groupes 2 et 3 repliés

Tout tient dans la hauteur ; l'Erreur caché dans le groupe replié reste visible dans son résumé.

```
┌──────────────────────────────────────────────────────┐
│ 4 Connectés · 1 en Reconnexion · 1 en Erreur     [+] │
│ ⌕ Filtrer…                                     [≡ ▾] │
├──────────────────────────────────────────────────────┤
│ ▾ eks-prod · payments                   8 · 3 actifs │
│ ● ledger-db:5432                         :5432  [ ●] │
│ ● ledger-ro:5433                         :5433  [ ●] │
│ ○ redis-cache:6379                       :6379  [● ] │
│ ○ kafka-ui:8080                          :8080  [● ] │
│ ↻ grafana:3000                           :3000  [ ●] │
│   recherche d'un pod Ready                           │
│ ○ prometheus:9090                        :9090  [● ] │
│ ○ vault:8200                             :8200  [● ] │
│ ○ jaeger:16686                          :16686  [● ] │
│ ▸ eks-staging · api                7 · ▲ 1 en Erreur │
│ ▸ docker-desktop · postiz                5 · 1 actif │
├──────────────────────────────────────────────────────┤
│ Tout désactiver                                  [⋯] │
└──────────────────────────────────────────────────────┘
```

Menu du filtre (glyphe `≡ ▾`) :

```
┌──────────────────────────────────────────────────────┐
│ Tous les Forwards            ✓                       │
│ Actifs seulement                                     │
│ En Reconnexion ou en Erreur                          │
└──────────────────────────────────────────────────────┘
```

Menu ⋯ :

```
┌──────────────────────────────────────────────────────┐
│ Réglages…                    ⌘,                      │
│ Ouvrir la config                                     │
│ Recharger la config          ⌘R                      │
├──────────────────────────────────────────────────────┤
│ Quitter Kubeport             ⌘Q                      │
└──────────────────────────────────────────────────────┘
```

### 3.3 État vide (aucun Forward configuré)

```
┌──────────────────────────────────────────────────────┐
│ Kubeport                                         [+] │
├──────────────────────────────────────────────────────┤
│                                                      │
│                                                      │
│                    ⇄                                 │
│                                                      │
│               Aucun Forward                          │
│                                                      │
│   Un Forward relie un port local à un Service ou     │
│   un Pod d'un cluster Kubernetes.                    │
│                                                      │
│           [  Créer un Forward  ]                     │
│                                                      │
│          Ou modifie le fichier de config             │
│              Ouvrir la config                        │
│                                                      │
├──────────────────────────────────────────────────────┤
│                                                  [⋯] │
└──────────────────────────────────────────────────────┘
```

### 3.4 Chargement au lancement (premier envoi du cœur pas encore reçu)

Le bouton + est désactivé tant que le cœur n'a pas répondu.

```
┌──────────────────────────────────────────────────────┐
│ Kubeport                                         [+] │
├──────────────────────────────────────────────────────┤
│                                                      │
│        Chargement des Forwards…                      │
│                                                      │
├──────────────────────────────────────────────────────┤
│                                                  [⋯] │
└──────────────────────────────────────────────────────┘
```

### 3.5 Erreur de démarrage (config illisible)

```
┌──────────────────────────────────────────────────────┐
│ Kubeport                                             │
├──────────────────────────────────────────────────────┤
│ ▲ Kubeport n'a pas pu lire sa config                 │
│                                                      │
│   expected `=` at line 12 column 5                   │
│   (config.toml)                                      │
│                                                      │
│   [ Ouvrir la config ]   [ Réessayer ]               │
│                                                      │
├──────────────────────────────────────────────────────┤
│                                                  [⋯] │
└──────────────────────────────────────────────────────┘
```

### 3.6 Lignes : détail ouvert, Erreur, Forward concurrent

```
┌──────────────────────────────────────────────────────┐
│ ● api:8080                               :8080  [ ●] │
│   Cible        svc/api:80                            │
│   Pod résolu   api-7d9f6c-x2k4j                      │
│   Adresse      localhost:8080            [Copier]    │
│   [Modifier…]  [Dupliquer…]                          │
├──────────────────────────────────────────────────────┤
│ ▲ pg-staging:5432                        :5432  [ ●] │
│   authentification refusée : token expiré            │
│   [Réessayer]  [Modifier…]                           │
├──────────────────────────────────────────────────────┤
│ ○ pg-replica:5433                        :5433  [● ] │
│   Forward concurrent : « ledger-ro:5433 » (même      │
│   port local). Un seul des deux est Actif.           │
│   [Modifier…]  [Dupliquer…]                          │
└──────────────────────────────────────────────────────┘
```

### 3.7 Formulaire (création, Services en cours de chargement)

```
┌──────────────────────────────────────────────────────┐
│ ‹ Forwards                       Nouveau Forward     │
├──────────────────────────────────────────────────────┤
│ Contexte                                             │
│ [ eks-staging                                  ▾ ]   │
│ Namespace                                            │
│ [ api                                          ▾ ]   │
│ Type                                                 │
│ [ Service | Pod ]                                    │
│ Service                                              │
│ [ ◌ Chargement des Services…                     ]   │
│ Port distant            Port local                   │
│ [ —        ▾ ]          [              ]             │
│   Par défaut : le port distant                       │
│ Nom                                                  │
│ [ api:80                                        ]    │
│   Proposé : cible:port                               │
├──────────────────────────────────────────────────────┤
│                             [Annuler]  [Enregistrer] │
└──────────────────────────────────────────────────────┘
```

Contexte injoignable (extrait) :

```
┌──────────────────────────────────────────────────────┐
│ ‹ Forwards                       Nouveau Forward     │
├──────────────────────────────────────────────────────┤
│ Contexte        [ eks-prod                   ▾ ]     │
│ Namespace       [ payments                     ]     │
│ ▲ Contexte « eks-prod » injoignable : délai dépassé  │
│   (10 s). Saisis le namespace à la main ou réessaie. │
│   [Réessayer]                                        │
│ …                                                    │
└──────────────────────────────────────────────────────┘
```

### 3.8 Réglages

```
┌──────────────────────────────────────────────────────┐
│ ‹ Forwards                                Réglages   │
├──────────────────────────────────────────────────────┤
│ [ ●] Lancer Kubeport à l'ouverture de session        │
│                                                      │
│ Kubeconfig                                           │
│ [ /Users/deki/.kube/config            ] [Choisir…]   │
│   Laisser vide : KUBECONFIG du shell, puis           │
│   ~/.kube/config.   [Appliquer]                      │
├──────────────────────────────────────────────────────┤
│ Kubeport 0.1.0                   Ouvrir la config    │
└──────────────────────────────────────────────────────┘
```

---

## 4. UX spec (gabarit de l'équipe)

### Flows

1. **Activer un Forward.** Ouvrir le panneau → repérer la ligne (recherche si > 10) → cliquer le switch (ou Espace) → le glyphe passe à `↻` avec « recherche d'un pod Ready » puis à `●`. Si un Forward concurrent était Actif : il repasse à `○` et un avis le dit.
2. **Vérifier que tout va bien.** Ouvrir le panneau → lire le résumé. Si une Erreur existe, la ligne (ou le titre du groupe replié) porte `▲` ; filtre « En Reconnexion ou en Erreur » en un clic.
3. **Traiter une Erreur.** Ligne en `▲` → lire la raison → « Réessayer » (cause passagère) ou « Modifier… » (port occupé, Cible introuvable) → Enregistrer (le Forward Actif est relancé).
4. **Créer un Forward.** + (ou ⌘N) → Contexte → namespace → Type → Cible → port distant (proposé) → port local (même valeur proposée) → Nom (proposé) → Enregistrer. Retour à la liste, ligne ajoutée à son groupe, **Inactive** (l'État voulu n'est pas Actif par défaut : à confirmer, question 5).
5. **Modifier ou dupliquer.** Clic sur la ligne → Détail → « Modifier… » ou « Dupliquer… » (aussi : clic droit, VoiceOver > Actions). Dupliquer propose le nom « {nom}-copie ».
6. **Supprimer.** Modifier → Supprimer → confirmation → Supprimer.
7. **Tout désactiver.** Pied → « Tout désactiver » → tous repassent Inactifs ; avis « 5 Forwards désactivés. Annuler » (annule en réactivant les mêmes noms, côté Swift).
8. **Réglages.** ⋯ → Réglages… (ou ⌘,) → le switch s'applique tout de suite ; kubeconfig → Appliquer.
9. **Config illisible.** Écran 3.5 → Ouvrir la config → corriger → Réessayer.
10. **Première utilisation.** État vide 3.3 → Créer un Forward → flux 4.

### Screen states

| Écran | Vide | Chargement | Erreur | Succès |
|---|---|---|---|---|
| Liste | 3.3 : texte d'explication + « Créer un Forward » + « Ouvrir la config » | 3.4 : « Chargement des Forwards… », + désactivé, aucun état vide affiché avant le premier envoi du cœur | 3.5 si le cœur n'a pas démarré ; ligne `▲` avec raison et actions pour un Forward en Erreur ; avis pour les erreurs ponctuelles | résumé d'état à jour, lignes `●` |
| Liste filtrée | « Aucun Forward ne correspond à « {texte} ». » + « Effacer la recherche » (ou « Tout afficher » si c'est le filtre d'état) | sans objet (filtrage local) | sans objet | groupes réduits aux correspondances |
| Détail d'une ligne | sans objet | sans objet | « Pod résolu » absent si pas Connecté | Cible, Pod résolu, Adresse, actions |
| Formulaire : Contexte | « Aucun Contexte dans le kubeconfig. » + « Ouvrir les Réglages » | liste locale, instantanée | Contexte absent (modification) : conservé avec « (absent du kubeconfig) » | Contexte sélectionné |
| Formulaire : namespace et Cible | « Aucun Service dans « {ns} ». » (+ note ExternalName) ; bascule vers Pod proposée | spinner dans le champ, champ désactivé, délai 10 s | message inline + « Réessayer » + saisie libre | liste remplie, premier élément sélectionné |
| Formulaire : enregistrement | Enregistrer grisé tant qu'incomplet, infobulle qui dit quoi | Enregistrer remplacé par spinner pendant `saving` | erreur du cœur sous le bouton, texte conservé | retour à la liste |
| Réglages | kubeconfig vide = défaut, placeholder explicite | lecture du kubeconfig : champ désactivé le temps de la lecture | erreur rouge sous le champ ; avis de lancement à l'ouverture de session | switch appliqué tout de suite ; « Kubeconfig appliqué. » |
| Menu ⋯ | « Recharger la config » grisé si le cœur n'a pas démarré (Réessayer à la place) | sans objet | sans objet | avis « Config relue. » (voir 5, Could) |

### Copy

Mots du glossaire respectés. Les `{…}` sont des valeurs. « Namespace » conservé tel quel.

**Liste et en-tête**

| Où | Texte |
|---|---|
| Résumé (items) | `{n} Connecté` / `{n} Connectés` · `{n} en Reconnexion` · `{n} en Erreur` |
| Résumé, aucun Actif | `Aucun Forward Actif` |
| Titre de groupe, résumé replié | `{n} · {m} actif` / `{n} · {m} actifs` · `▲ {k} en Erreur` · `↻ {k} en Reconnexion` |
| Bouton + (étiquette accessible, infobulle) | `Nouveau Forward` |
| Champ de recherche, placeholder | `Filtrer` |
| Menu du filtre | `Tous les Forwards` · `Actifs seulement` · `En Reconnexion ou en Erreur` |
| Puce du filtre | `Actifs` · `Reconnexion ou Erreur` |
| Aucun résultat | `Aucun Forward ne correspond à « {texte} ».` / `Aucun Forward Actif.` / `Aucun Forward en Reconnexion ou en Erreur.` |
| Boutons de l'état vide filtré | `Effacer la recherche` · `Tout afficher` |
| Pied | `Tout désactiver` |
| Menu ⋯ | `Réglages…` · `Ouvrir la config` · `Recharger la config` · `Quitter Kubeport` |

**Ligne de Forward**

| Où | Texte |
|---|---|
| Switch, étiquette | `Activer le Forward {nom}` |
| Switch, valeur | `Actif` / `Inactif` |
| Étiquette VoiceOver de la ligne | `{nom}, {Connecté / Reconnexion / Erreur / Inactif}, port local {port}` |
| Sous-ligne Reconnexion / Erreur | la raison renvoyée par le cœur, telle quelle |
| Détail : libellés | `Cible` · `Pod résolu` · `Adresse` |
| Détail : valeurs | `svc/{nom}:{port}` ou `pod/{nom}:{port}` · `{pod}` · `localhost:{port}` |
| Détail : boutons | `Copier` (puis `Copié` 2 s) · `Modifier…` · `Dupliquer…` |
| Actions d'une ligne en Erreur | `Réessayer` · `Modifier…` |
| Forwards concurrents | `Forward concurrent : « {autre} » utilise le même port local. Un seul des deux est Actif.` |
| Menu clic droit / actions VoiceOver | `Modifier…` · `Dupliquer…` · `Copier l'adresse` · `Réessayer` (si Erreur) |
| Infobulle nom tronqué | `{nom complet}` |

**États vides et d'erreur**

| Où | Texte |
|---|---|
| Vide : titre | `Aucun Forward` |
| Vide : explication | `Un Forward relie un port local à un Service ou à un Pod d'un cluster Kubernetes.` |
| Vide : boutons | `Créer un Forward` · `Ouvrir la config` |
| Chargement | `Chargement des Forwards…` |
| Erreur de démarrage : titre | `Kubeport n'a pas pu lire sa config` |
| Erreur de démarrage : corps | la raison du cœur, en police à chasse fixe, sélectionnable |
| Erreur de démarrage : boutons | `Ouvrir la config` · `Réessayer` |

**Avis (texte final)**

| Situation | Texte |
|---|---|
| Forward concurrent désactivé (cœur : `{rival} désactivé : port local partagé avec {name}`) | conservé tel quel côté cœur ; l'interface met en gras les deux noms |
| Tout désactiver | `{n} Forward désactivé.` / `{n} Forwards désactivés.` + bouton `Annuler` |
| Config non rechargée (cœur) | `config non rechargée : {raison}` tel quel |
| Config relue (Could, nécessite le cœur) | `Config relue : {n} Forwards.` |
| Lancement à l'ouverture de session refusé | `Lancement à l'ouverture de session : {raison}` |
| Copie | `Adresse copiée.` |
| Fermeture de l'avis (accessibilité) | `Fermer l'avis` |

**Formulaire**

| Où | Texte |
|---|---|
| Retour | `‹ Forwards` (étiquette accessible : `Retour à la liste des Forwards`) |
| Titres | `Nouveau Forward` · `Modifier le Forward` · `Dupliquer le Forward` |
| Champs | `Contexte` · `Namespace` · `Type` (`Service` / `Pod`) · `Service` ou `Pod` · `Port distant` · `Port local` · `Nom` |
| Chargement | `Chargement des namespaces…` · `Chargement des Services…` · `Chargement des Pods…` |
| Aide port local | `Par défaut : le port distant.` |
| Aide nom | `Proposé : cible:port.` |
| Aucun Contexte | `Aucun Contexte dans le kubeconfig. Ajoute-en un avec kubectl, ou choisis un autre kubeconfig dans les Réglages.` + `Ouvrir les Réglages` |
| Contexte absent | `{contexte} (absent du kubeconfig)` + `Ce Contexte n'existe plus dans le kubeconfig : choisis-en un autre.` |
| Cluster injoignable | `Contexte « {contexte} » injoignable : {raison}. Saisis le namespace à la main ou réessaie.` (idem avec `la Cible`) + `Réessayer` |
| Délai dépassé | `Contexte « {contexte} » injoignable : délai dépassé (10 s). …` |
| Aucun namespace listé (droits) | `Tu n'as pas le droit de lister les namespaces : saisis-le à la main.` |
| Aucune Cible | `Aucun Service dans « {namespace} ».` + `Les Services ExternalName et sans selector ne sont pas proposés.` + bouton `Choisir un Pod` |
| Nom déjà pris | `Un Forward porte déjà ce nom.` |
| Port invalide | `Entre un port entre 1 et 65535.` |
| Forwards concurrents (info) | `Forward concurrent : « {autre} » utilise aussi le port {port}. Un seul des deux sera Actif.` |
| Forward Actif en modification | `Ce Forward est Actif : il sera relancé à l'enregistrement.` |
| Enregistrer grisé (infobulle) | `Complète : {liste des champs manquants}.` |
| Boutons | `Annuler` · `Enregistrer` |
| Suppression | `Supprimer` → ligne : `Supprimer « {nom} » ?` + `Annuler` · `Supprimer` ; si Actif : `Il sera d'abord arrêté.` |
| Erreur du cœur à l'enregistrement | la raison du cœur sous les boutons |

**Réglages**

| Où | Texte |
|---|---|
| Titre | `Réglages` |
| Switch | `Lancer Kubeport à l'ouverture de session` |
| À autoriser | `macOS demande ton accord.` + bouton `Ouvrir Réglages Système` (`SMAppService.openSystemSettingsLoginItems()`) |
| Champ | `Kubeconfig` ; placeholder `Par défaut : KUBECONFIG du shell, puis ~/.kube/config` |
| Boutons | `Choisir…` · `Appliquer` |
| Aide | `Laisse vide pour utiliser le kubeconfig par défaut.` |
| Pied | `Kubeport {version}` · `Ouvrir la config` |
| Succès | `Kubeconfig appliqué.` |

**Icône de la barre de menu (étiquette accessible et infobulle)**

`Kubeport : aucun Forward Actif` · `Kubeport : {n} Forward Actif` / `{n} Forwards Actifs` · `Kubeport : {n} Forwards Actifs, {k} en Erreur`

### Accessibility

- Chaque ligne de Forward est un conteneur VoiceOver (`.accessibilityElement(children: .contain)`) dont l'étiquette est `{nom}, {état}, port local {port}` ; le point de couleur est masqué à VoiceOver.
- Le switch porte l'étiquette `Activer le Forward {nom}` et la valeur Actif/Inactif.
- Actions personnalisées VoiceOver sur chaque ligne : Activer/Désactiver, Modifier, Dupliquer, Copier l'adresse, Réessayer (si Erreur).
- L'état n'est jamais porté par la couleur seule : forme du glyphe distincte pour les quatre états, plus le texte du résumé et la raison.
- Les titres de groupe sont des boutons : étiquette `{contexte} · {namespace}, {n} Forwards, {m} actifs, replié` / `déplié`.
- Navigation complète au clavier avec le tableau 2.6, **sans** l'option système « Accès au clavier complet ».
- Un focus visible sur la ligne, le titre de groupe et le champ de recherche (anneau ou surbrillance, contraste ≥ 3:1 avec le fond).
- Contrastes : texte ≥ 4,5:1, glyphes d'état ≥ 3:1 en clair et en sombre, avec « Augmenter le contraste » activé.
- Polices sémantiques (`.body`, `.caption`), pas de taille fixe. Aucune animation de rotation ; si une animation est ajoutée, elle respecte « Réduire les animations ».
- Cibles cliquables ≥ 24 × 24 pt (switch `.small`, boutons de texte avec marge de 4 pt).
- Chaque avis est annoncé à VoiceOver, et le bouton `Fermer l'avis` est atteignable.
- Dans le formulaire : chaque champ a une étiquette, les messages inline sont liés à leur champ (`accessibilityHint` ou même conteneur), l'erreur de validation est annoncée à l'apparition.

### Sketch

Voir la section 3 (maquettes 3.1 à 3.8).

### DoD additions

Tests unitaires Swift (cible `KubeportTests`, à créer) sauf mention.

- [ ] Le modèle ne produit pas l'état vide avant le premier `forwardsChanged` : test sur `CoreModel.hasLoaded` (faux au départ, vrai après le premier événement) ; l'écran liste affiche « Chargement des Forwards… » tant qu'il est faux.
- [ ] Avec `startError`, « Ouvrir la config » ouvre `config.toml` (chemin calculé sans `core`) et « Réessayer » recrée le cœur ; test sur `CoreModel.retryStart()` avec une config valide.
- [ ] Les groupes, les compteurs du résumé et les résumés de groupes replié sont calculés dans `CoreModel` à chaque `forwardsChanged` ; test sur 20 Forwards en 3 groupes : compteurs attendus (4 Connectés, 1 en Reconnexion, 1 en Erreur, groupe replié `▲ 1 en Erreur`).
- [ ] La recherche filtre sur nom, Contexte, namespace, Cible et port local ; test sur la fonction de filtre ; un groupe replié avec correspondance est affiché déplié sans modifier la préférence mémorisée.
- [ ] La recherche et le filtre d'état apparaissent à partir de 11 Forwards et pas avant ; test sur la constante `compactLimit = 10`.
- [ ] La zone de liste mesure 440 pt de haut à partir de 11 Forwards (XCUITest ou test de présentation, avec 20 Forwards) et ne change pas de hauteur quand on filtre ou replie un groupe.
- [ ] Les titres de groupe restent épinglés au défilement et la barre de défilement est visible (XCUITest : `scrollBars.firstMatch` existe et est visible avec 20 Forwards).
- [ ] Un Forward en Erreur dans un groupe replié rend visible `▲ {k} en Erreur` dans le titre du groupe.
- [ ] Chaque ligne a pour étiquette `{nom}, {état}, port local {port}` et son switch `Activer le Forward {nom}` ; Accessibility Inspector > Audit ne remonte aucun problème sur l'écran liste.
- [ ] Les quatre états ont des glyphes de formes distinctes (test sur la fonction `glyph(for:)`) ; captures du panneau en clair et en sombre relues pour le contraste ≥ 3:1 des glyphes.
- [ ] ↑ / ↓ déplacent le focus, Espace bascule l'État voulu, Entrée ouvre le détail, ⌘F focalise la recherche, Échap ferme dans l'ordre détail → recherche → panneau ; XCUITest avec « Accès au clavier complet » désactivé.
- [ ] Clic sur une ligne : le détail montre Cible, Adresse, « Modifier… », « Dupliquer… » et, si Connecté, Pod résolu ; un second clic ou un autre détail ouvert ferme le premier.
- [ ] Un Forward en Erreur affiche « Réessayer » et « Modifier… » ; « Réessayer » enchaîne `deactivate` puis `activate` du même nom (test avec cœur factice).
- [ ] L'avis s'affiche depuis les trois écrans (liste, formulaire, réglages) ; il est fermable par ✕ ; sa durée suit `max(6 s, 0,06 s × caractères)` plafonnée à 20 s ; un avis est annoncé par VoiceOver (test sur la publication `AccessibilityNotification.Announcement`).
- [ ] Formulaire : un chargement de namespaces ou de Cibles affiche le spinner, désactive le champ, ignore une réponse périmée (test : deux chargements en course, la première réponse arrive en dernier), et passe en message inline + « Réessayer » au bout de 10 s (test avec horloge injectée).
- [ ] Formulaire : « Un Forward porte déjà ce nom. » apparaît avant Enregistrer ; port hors 1–65535 refusé ; l'information « Forward concurrent » apparaît quand le port local est partagé.
- [ ] Formulaire : la suppression passe par la ligne de confirmation, le focus initial est sur « Annuler », la confirmation se referme au bout de 5 s ; un double-clic sur « Supprimer » ne supprime pas.
- [ ] Réglages : le switch de lancement s'applique sans Enregistrer ; le champ Kubeconfig a son bouton « Appliquer ».
- [ ] L'icône a une étiquette accessible dynamique pour les trois états (test sur `StatusIcon.description(activeCount:errorCount:)`) ; l'état Erreur est lisible sur barre claire et sombre (captures manuelles jointes à la PR).
- [ ] À chaque ouverture du panneau : retour à la liste, recherche vide, filtre « Tous les Forwards », aucun détail ouvert (vérifié à la main : fermer par clic extérieur avec un formulaire ouvert, rouvrir).
- [ ] Mémoire : `scripts/measure-footprint.sh` avec 5 Forwards Actifs reste ≤ 35 Mo, puis avec une config de 50 Forwards et le panneau ouvert et défilé ; le résultat est noté dans la PR.
- [ ] Ouverture : avec 50 Forwards, l'ouverture du panneau ne dépasse pas 100 ms sur le fil principal (Instruments, Time Profiler ; chiffre noté dans la PR) ; aucun appel du cœur dans le chemin d'ouverture.
- [ ] Aucun texte d'interface ne contient un des mots proscrits par `GLOSSARY.md` (test de recherche dans les fichiers Swift : tunnel, statut, échec, conflit de port, doublon, pod actif, on/off).

### Open questions

1. **Reconnexion à la première connexion.** Activer un Forward affiche d'abord `Reconnexion` (raison « recherche d'un pod Ready »). Le glossaire définit Reconnexion comme un état qui suit une perte. Deux voies : (a) accepter, le glyphe `↻` et la raison suffisent ; (b) ajouter un état « Connexion » au glossaire et à `RealStateView` (changement du cœur et de l'ADR). Recommandation : (a) pour la v0.2, et poser la question à Dodie pour le glossaire.
2. **Clic sur la ligne pour ouvrir le détail.** Alternative : bouton `⋯` visible au survol et au focus. Recommandation : clic sur la ligne (zéro pixel ajouté, une seule cible), à valider une fois l'interface vue à l'écran.
3. **Seuil de 10 Forwards** pour la recherche et la hauteur fixe, et hauteur de liste de 440 pt. Valeurs à ajuster après la première vue réelle.
4. **Changer le kubeconfig avec des Forwards Actifs** : sont-ils relancés ? Le comportement n'est pas visible dans l'interface. Si oui, ajouter « Les Forwards Actifs seront relancés. » sous le champ ; règle métier à confirmer par Dodie.
5. **Un Forward créé est-il Actif ou Inactif ?** Le code actuel enregistre sans activer. Une case « Activer après l'enregistrement » est possible ; à trancher par le propriétaire.
6. **Filtre d'état remis à « Tous » à chaque ouverture** : choix prudent. Mémoriser la dernière valeur est possible mais fait paraître des Forwards « disparus ».
7. **Raccourcis ⌘N, ⌘R, ⌘,** : ils ne fonctionnent que si le panneau a le focus, ce qui est le cas à l'ouverture. À confirmer au rendu qu'ils ne gênent pas les raccourcis système.

---

## 5. Changements priorisés

Estimation de taille : S (< ½ jour), M (≈ 1 jour), L (plusieurs jours).

### Must : à faire avant de montrer l'app à d'autres

| # | Changement | Fichiers | Taille | Cœur / FFI |
|---|---|---|---|---|
| M1 | Indicateur `hasLoaded` : « Chargement des Forwards… » au lieu du faux état vide ; + désactivé tant que faux | `CoreModel.swift`, `PanelView.swift` | S | non |
| M2 | Erreur de démarrage utile : « Ouvrir la config » et « Réessayer » fonctionnent sans cœur | `CoreModel.swift`, `PanelView.swift` | S | **optionnel** : exposer `default_config_path()` en fonction libre FFI. Sans cela, dupliquer le chemin dans Swift (risque de dérive avec `Config::default_path`) |
| M3 | Accessibilité des lignes : étiquettes, glyphes à forme distincte, switch `.small` nommé, état jamais en couleur seule | `PanelView.swift` (`ForwardRow`) | S | non |
| M4 | Liste à 20+ : `LazyVStack` + titres épinglés, `.scrollIndicators(.visible)`, ligne de 28 pt (sous-ligne seulement pour Reconnexion et Erreur), hauteur stable | `PanelView.swift` (`ListScreen`, `GroupSection`, `ForwardRow`) | M | non |
| M5 | Groupes repliables avec résumé d'état (aucun Erreur caché) ; état mémorisé | `PanelView.swift`, `CoreModel.swift` (groupes et compteurs dérivés) | M | non |
| M6 | Résumé d'état en en-tête + bouton + dans l'en-tête | `PanelView.swift` | S | non |
| M7 | Recherche et filtre d'état au-delà de 10 Forwards | `PanelView.swift`, `CoreModel.swift` | M | non |
| M8 | Détail de ligne (clic) avec Cible, Pod résolu, Adresse, Modifier…, Dupliquer… ; actions VoiceOver ; clic droit conservé | `PanelView.swift` | M | non |
| M9 | Actions sur une ligne en Erreur : « Réessayer », « Modifier… » | `PanelView.swift`, `CoreModel.swift` (`retry(_:)`) | S | non |
| M10 | Avis au niveau de `PanelView` (tous les écrans), fermable, annoncé à VoiceOver, durée adaptée | `PanelView.swift`, `CoreModel.swift` | S | non |
| M11 | Formulaire : chargement par champ, réponses périmées ignorées, délai de 10 s, messages inline, cas Aucun Contexte / Aucune Cible / Contexte absent | `ForwardForm.swift` | M | **recommandé** : délai d'abandon dans `catalog::namespaces/targets`. Sans cela, course de délai côté Swift (`withTaskGroup`) : la requête continue en fond mais l'interface est libre |
| M12 | Suppression : ligne de confirmation distincte, focus sur Annuler, refermeture à 5 s | `ForwardForm.swift` | S | non |

### Should : juste après

| # | Changement | Fichiers | Taille | Cœur / FFI |
|---|---|---|---|---|
| S1 | Pied réduit : « Tout désactiver » + menu ⋯ (Réglages, config, Recharger, Quitter) | `PanelView.swift` (`Footer`) | S | non |
| S2 | Navigation clavier complète (↑ ↓ Espace Entrée ⌘F ⌘N ⌘, ⌘R, Échap en cascade) | `PanelView.swift` | M | non (macOS 14 : `onKeyPress`) |
| S3 | Retour à la liste, recherche vide et filtre remis à zéro à chaque ouverture du panneau | `PanelView.swift` | S | non (à vérifier : `onAppear` n'est pas rappelé à chaque ouverture en `.window` ; observer `NSWindow.didBecomeKeyNotification`) |
| S4 | Formulaire : validation inline (nom pris, port 1–65535), information Forwards concurrents, avertissement « sera relancé » | `ForwardForm.swift` | S | non |
| S5 | Réglages : switch immédiat, « Appliquer » propre au kubeconfig, « Choisir… », lien vers Réglages Système, version | `SettingsView.swift` | S | non |
| S6 | Icône : `accessibilityDescription` dynamique ; vérifier l'état Erreur sur barres claire et sombre, repli template si illisible | `StatusIcon.swift`, `KubeportApp.swift` | S | non |
| S7 | Phrase « Forward concurrent » dans le détail d'une ligne dont le port local est partagé | `PanelView.swift` | S | non |

### Could : plus tard

| # | Changement | Cœur / FFI |
|---|---|---|
| C1 | Avis « Config relue : N Forwards. » après Recharger | **Oui** : `Command::Reload` ne notifie que l'échec |
| C2 | Gravité des avis (information / erreur), pour garder les erreurs jusqu'à fermeture | **Oui** : `notice(message)` est une chaîne seule |
| C3 | État « Connexion » distinct de « Reconnexion » | **Oui** : `RealStateView` et glossaire (question 1) |
| C4 | Pré-contrôle « port local déjà occupé par un autre processus » dans le formulaire | **Oui** (ou Swift via `bind` d'essai) |
| C5 | Annuler « Tout désactiver » (réactive les mêmes noms) | non |
| C6 | Raccourcir l'affichage des Contextes longs (`arn:aws:eks:…/prod` → `prod`, complet en infobulle) | non |
| C7 | Action de groupe (désactiver tous les Forwards du groupe) dans le menu du titre | non |
| C8 | Sélecteur de namespace ou de Cible avec recherche dans le formulaire (clusters de centaines de namespaces) | non |
| C9 | « Réessayer tout » dans l'en-tête quand plusieurs Forwards sont en Erreur | non |

### Ce que ces changements ne coûtent pas

Pas de dépendance ajoutée, pas de webview, pas de timer, pas de polling : conforme aux ADR 0002 et 0003. Les seules données ajoutées sont des compteurs dérivés en mémoire et un tableau de noms de groupes dans `UserDefaults`. Aucun changement du cœur n'est obligatoire : les Could C1 à C4 en demandent, et M2 et M11 ont une alternative purement Swift.
