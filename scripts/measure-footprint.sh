#!/usr/bin/env bash
# Mesure l'empreinte mémoire physique de Kubeport en cours d'exécution.
# Budget : <= 35 Mo au repos, panneau fermé, 5 Forwards Actifs.
set -euo pipefail

BUDGET_MB=35
PID="$(pgrep -x Kubeport || true)"
if [[ -z "$PID" ]]; then
  echo "Kubeport n'est pas lancé." >&2
  exit 2
fi

BYTES="$(footprint -p "$PID" --format bytes 2>/dev/null | awk '/phys_footprint:/ {print $2; exit}')"
if [[ -z "$BYTES" ]]; then
  # Repli : RSS en Ko via ps (moins précis que phys_footprint).
  BYTES="$(( $(ps -o rss= -p "$PID") * 1024 ))"
fi

MB="$(( BYTES / 1024 / 1024 ))"
echo "Kubeport (pid $PID) : ${MB} Mo (budget ${BUDGET_MB} Mo)"
(( MB <= BUDGET_MB ))
