# Deploying Opake

Kustomize manifests for the hosted Opake instances. The cluster itself
(Kapsule, Traefik, cert-manager, PostgreSQL, namespaces, deploy RBAC) is
defined in the private Infrastructure repo; these manifests assume that
foundation and reference its Secrets by name only — no secret material lives
in this repo.

```
deploy/k8s/
  base/           web (static frontend), indexer (Elixir, Phoenix)
  overlays/
    staging/      staging.opake.at + indexer.staging.opake.at, basic-auth gated
    prod/         opake.at + indexer.opake.at, plus the opake.app → opake.at
                  permanent redirect (domain being phased out)
```

Images are built for amd64 by the GitHub Actions workflows and pushed to
`rg.fr-par.scw.cloud/opake/{web,indexer}`. Staging deploys on every push to
`main`; prod deploys on release tags (workflow lands with the first release).

Expected Secrets per namespace (rendered by CI from Actions secrets):
`opake-database-url` (`url`), `opake-secrets` (`secret-key-base`), and in
staging additionally `staging-basicauth` (`users`, htpasswd format).

Self-hosters: the overlays are thin — copy one, change the hosts and the
image registry, point `opake-database-url` at your own PostgreSQL. A
reproducible single-host deployment (NixOS modules) is planned separately.

Verify locally with `kubectl kustomize deploy/k8s/overlays/staging`.
