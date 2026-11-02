# Kubeport

Outil de barre de menu macOS qui maintient des port-forwards Kubernetes configurés une fois, activables d'un clic, et qui suivent automatiquement les pods derrière un Service.

## Language

### Configuration

**Forward**:
Un port-forward configuré et enregistré, désigné par un nom : une Cible, une seule paire port distant → port local, rattachés à un Contexte et un namespace figés.
_Avoid_: PF, tunnel, redirection, binding

**Cible**:
La ressource Kubernetes visée par un Forward : un Service (cas principal) ou un Pod nommé (cas de secours).
_Avoid_: target, destination, backend

**Contexte**:
Le contexte kubeconfig (cluster + utilisateur) figé dans un Forward, indépendant du `current-context` du terminal.
_Avoid_: cluster, environnement

**Forwards concurrents**:
Des Forwards qui partagent le même port local ; au plus un d'entre eux est Actif à la fois.
_Avoid_: conflit de port, doublon

### Exécution

**Pod résolu**:
Le pod qui reçoit les nouvelles connexions d'un Forward. Pour une Cible Service, c'est le plus ancien pod Ready au moment de la résolution ; il ne change que lorsqu'il cesse d'être Ready.
_Avoid_: pod actif, pod courant, endpoint

**État voulu**:
Ce que l'utilisateur demande pour un Forward via le bouton : **Actif** ou **Inactif**.
_Avoid_: enabled, on/off, statut

**État réel**:
Ce qui se passe effectivement pour un Forward Actif : **Connecté**, **Reconnexion** ou **Erreur**. Un Forward Actif peut ne pas être Connecté.
_Avoid_: statut, santé

**Connecté**:
État réel d'un Forward Actif dont le port local écoute et qui a un Pod résolu.
_Avoid_: up, en ligne

**Reconnexion**:
État réel d'un Forward Actif qui a perdu son Pod résolu, son tunnel ou le réseau, et réessaie seul tant qu'il reste Actif.
_Avoid_: retry, failover

**Erreur**:
État réel d'un Forward Actif bloqué par une cause qui demande une action humaine (port local occupé, authentification refusée, Cible ou namespace introuvable, Contexte absent) ; aucune nouvelle tentative automatique.
_Avoid_: échec, crash
