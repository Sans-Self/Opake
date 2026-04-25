import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/devices/oauth-callback")({
  // No boot here — completeLogin handles the fresh OAuth code exchange.
  // Booting with a stale session would consume the old refresh token
  // and redirect before completeLogin runs, wasting the new auth code.
});
