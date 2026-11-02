# The app resolves the pod behind a Service itself, connection by connection

For a Service Target, the app reads the Service selector, watches only the pods that match it (a label-filtered watch), and picks the oldest Ready pod as the Resolved pod. It watches pods rather than EndpointSlices: a single watch provides readiness, the `Terminating` state, the creation time (for "oldest") and the containers' named ports. Each local TCP connection opens its own port-forward stream to the current Resolved pod. When that pod turns `Terminating` or is no longer Ready, new connections go to another Ready pod, and existing connections stay on the old one until it dies. As long as the Resolved pod is healthy, the app never switches.

We do not use the equivalent of `kubectl port-forward svc/...`: it picks a pod once and dies with it, never switching, which makes every rollout disruptive.

## Consequences

- The app must map the Service port to the container's `targetPort` itself, named ports included, for each Resolved pod.
- `ExternalName` Services and Services without a selector cannot be forwarded: they are hidden at creation time and go to Error if they become so later.
