# L'app résout elle-même le pod derrière un Service, connexion par connexion

Pour une Cible Service, l'app lit le selector du Service, surveille uniquement les pods qui y correspondent (un watch filtré par labels), et choisit le plus ancien pod Ready comme Pod résolu. On surveille les pods plutôt que les EndpointSlices : un seul watch fournit à la fois la readiness, l'état `Terminating`, la date de création (pour « le plus ancien ») et les ports nommés des conteneurs. Chaque connexion TCP locale ouvre son propre flux de port-forward vers le Pod résolu du moment. Quand ce pod passe `Terminating` ou n'est plus Ready, les nouvelles connexions partent vers un autre pod Ready, et les connexions existantes restent sur l'ancien jusqu'à sa mort. Tant que le Pod résolu est sain, on ne bascule jamais.

On n'utilise pas l'équivalent de `kubectl port-forward svc/...` : il choisit un pod une seule fois puis meurt avec lui, sans jamais basculer, ce qui rend chaque rollout bloquant.

## Consequences

- L'app doit traduire elle-même le port du Service en `targetPort` du conteneur, y compris les ports nommés, pour chaque Pod résolu.
- Les Services `ExternalName` et sans selector ne sont pas forwardables : ils sont masqués à la création et passent en Erreur s'ils le deviennent ensuite.
