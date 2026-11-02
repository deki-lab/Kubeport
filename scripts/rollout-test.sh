#!/usr/bin/env bash
# End-to-end test on a local cluster (default: docker-desktop).
# Activates the "whoami" Forward, sends requests in a loop during a rollout,
# then checks that traffic moved to the new pods.
set -euo pipefail

CTX="${KUBEPORT_IT_CONTEXT:-docker-desktop}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$(mktemp -t kubeport-it)"
K=(kubectl --context "$CTX")

"${K[@]}" apply -f "$ROOT/tests/fixtures/whoami.yaml" >/dev/null
"${K[@]}" -n kubeport-it rollout status deploy/whoami --timeout=120s >/dev/null

cargo build -q -p kubeport-cli --manifest-path "$ROOT/Cargo.toml"
"$ROOT/target/debug/kubeport" --config "$ROOT/tests/fixtures/config.toml" up whoami >"$LOG" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true' EXIT

for _ in $(seq 50); do grep -q 'Connected' "$LOG" && break; sleep 0.2; done
grep -q 'Connected' "$LOG" || { echo "FAIL: never Connected"; cat "$LOG"; exit 1; }

before="$(curl -sf --max-time 2 http://127.0.0.1:18080/ | awk '/^Hostname:/ {print $2}')"
echo "before rollout: $before"

"${K[@]}" -n kubeport-it rollout restart deploy/whoami >/dev/null
ok=0; fail=0
end=$((SECONDS + 40))
while (( SECONDS < end )); do
  if curl -sf --max-time 2 -o /dev/null http://127.0.0.1:18080/; then ok=$((ok+1)); else fail=$((fail+1)); fi
  sleep 0.1
done
"${K[@]}" -n kubeport-it rollout status deploy/whoami --timeout=120s >/dev/null

after="$(curl -sf --max-time 2 http://127.0.0.1:18080/ | awk '/^Hostname:/ {print $2}')"
echo "after rollout: $after"
echo "requests during the rollout: $ok ok, $fail failed"
echo "--- Actual states ---"; cat "$LOG"

[[ -n "$after" && "$after" != "$before" ]] || { echo "FAIL: no switchover"; exit 1; }
(( fail * 20 <= ok + fail )) || { echo "FAIL: more than 5% failures"; exit 1; }
echo "OK"
