import { CheckIcon } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";

type CallbackState = "loading" | "success" | "error";

function OAuthCallbackPage() {
  const [state, setState] = useState<CallbackState>("loading");
  const [errorMessage, setErrorMessage] = useState<string>("");

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const error = params.get("error");

    // Strip query params from URL
    window.history.replaceState({}, "", window.location.pathname);

    if (error) {
      setErrorMessage(error);
      setState("error");
      return;
    }

    // Just show success - CLI login doesn't log you into web
    setState("success");
  }, []);

  return (
    <div className="flex min-h-screen items-center justify-center bg-base-300 font-sans">
      <div className="flex w-full max-w-md flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />

        {state === "loading" && (
          <div className="flex flex-col items-center gap-4">
            <span className="loading loading-spinner loading-lg text-primary" />
            <p className="text-sm text-base-content/60">Completing authentication…</p>
          </div>
        )}

        {state === "success" && (
          <div className="flex flex-col items-center gap-6 text-center">
            <div className="flex h-16 w-16 items-center justify-center rounded-full bg-success/20">
              <CheckIcon size={32} />
            </div>

            <div className="flex flex-col gap-2">
              <h1 className="text-2xl font-semibold text-base-content">
                CLI login successful
              </h1>
            </div>

            <div className="mt-2 flex flex-col gap-3 text-sm text-base-content/50">
              <p>
                You can close this tab and return to your terminal.
              </p>
              <p>
                <span className="font-medium text-base-content/70">Note:</span> This
                logs you into the CLI only. The web app requires a separate login.
              </p>
            </div>
          </div>
        )}

        {state === "error" && (
          <div className="flex flex-col items-center gap-6 text-center">
            <div className="flex h-16 w-16 items-center justify-center rounded-full bg-error/20">
              <svg
                className="h-8 w-8 text-error"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </div>

            <div className="flex flex-col gap-2">
              <h1 className="text-2xl font-semibold text-base-content">
                CLI login failed
              </h1>
              <p className="text-base-content/60">{errorMessage}</p>
            </div>

            <div className="mt-2 flex flex-col gap-3 text-sm text-base-content/50">
              <p>
                Please try again from your terminal.
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export const Route = createFileRoute("/oauth/cli-callback")({
  component: OAuthCallbackPage,
});
