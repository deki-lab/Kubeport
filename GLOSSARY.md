# Kubeport

macOS menu bar tool that keeps Kubernetes port-forwards configured once, switched on with one click, and that automatically follow the pods behind a Service.

## Language

### Configuration

**Forward**:
A saved, named port-forward: one Target and a single remote port → local port pair, bound to a fixed Context and namespace.
_Avoid_: PF, tunnel, redirect, binding

**Target**:
The Kubernetes resource a Forward points to: a Service (main case) or a named Pod (fallback).
_Avoid_: destination, backend

**Context**:
The kubeconfig context (cluster + user) fixed in a Forward, independent of the terminal's `current-context`.
_Avoid_: cluster, environment

**Idle timeout**:
How long an Active Forward can go without any open connection before it turns Inactive on its own; set as a default and overridable per Forward ("Never" allowed).
_Avoid_: timeout, auto-disconnect, expiration

**Competing Forwards**:
Forwards that share the same local port; at most one of them is Active at a time.
_Avoid_: port conflict, duplicate

### Runtime

**Resolved pod**:
The pod that receives a Forward's new connections. For a Service Target, it is the oldest Ready pod at resolution time; it changes only when it stops being Ready.
_Avoid_: active pod, current pod, endpoint

**Desired state**:
What the user asks of a Forward through its toggle: **Active** or **Inactive**.
_Avoid_: enabled, on/off, status

**Actual state**:
What is really happening to an Active Forward: **Connected**, **Reconnecting** or **Error**. An Active Forward is not necessarily Connected.
_Avoid_: status, health

**Connected**:
Actual state of an Active Forward whose local port is listening and that has a Resolved pod.
_Avoid_: up, online

**Reconnecting**:
Actual state of an Active Forward that lost its Resolved pod, its tunnel or the network, and retries on its own as long as it stays Active.
_Avoid_: retry, failover

**Error**:
Actual state of an Active Forward blocked by a cause that needs human action (local port in use, authentication denied, Target or namespace not found, missing Context); no automatic retry.
_Avoid_: failure, crash

## French UI

The UI follows the macOS language: English or French (English for any other language). Each term as shown in the French UI:

- Forward → Forward
- Target → Cible
- Context → Contexte
- Resolved pod → Pod résolu
- Active / Inactive → Actif / Inactif
- Connected / Reconnecting / Error → Connecté / Reconnexion / Erreur
- Competing Forwards → Forwards concurrents
- Idle timeout → Délai d'inactivité
