#!/usr/bin/env bash
# Measures the physical memory footprint of the running Kubeport.
# Budget: <= 35 MB at rest, panel closed, 5 Active Forwards.
set -euo pipefail

BUDGET_MB=35
PID="$(pgrep -x Kubeport || true)"
if [[ -z "$PID" ]]; then
  echo "Kubeport is not running." >&2
  exit 2
fi

BYTES="$(footprint -p "$PID" --format bytes 2>/dev/null | awk '/phys_footprint:/ {print $2; exit}')"
if [[ -z "$BYTES" ]]; then
  # Fallback: RSS in KB via ps (less accurate than phys_footprint).
  BYTES="$(( $(ps -o rss= -p "$PID") * 1024 ))"
fi

MB="$(( BYTES / 1024 / 1024 ))"
echo "Kubeport (pid $PID): ${MB} MB (budget ${BUDGET_MB} MB)"
(( MB <= BUDGET_MB ))
