// Vitest globalSetup: provision the run's actor namespace once, before any
// federation spec touches the CLI. The tier's actors resolve through
// `devenvActors()` (helpers/pds.ts), which under E2E_ACTOR_NS returns six
// derived actors that may not exist on the dev-env yet.
//
// It lives in globalSetup rather than a per-file `beforeAll` because vitest runs
// federation files in parallel worker processes: two workers first-provisioning
// the same namespace would race each other into duplicate account creation.
// Once here, idempotently, is the only safe shape.
//
// No-op for the default population (checked-in actors, bootstrapped with the
// dev-env) and for the fake-pds tier.

import { actorNamespace } from "../e2e/namespace.js";
import { provisionNamespace } from "../e2e/pds-admin.js";

export default async function setup(): Promise<void> {
  if (process.env.OPAKE_TEST_ENV !== "devenv") return;
  const ns = actorNamespace();
  if (ns === "") return;
  const provisioned = await provisionNamespace(ns);
  console.log(
    provisioned.length > 0
      ? `[federation] provisioned namespace ${ns}: ${provisioned.join(", ")}`
      : `[federation] namespace ${ns} already provisioned`,
  );
}
