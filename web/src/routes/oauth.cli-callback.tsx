import { CheckIcon } from "@phosphor-icons/react"
import { useMemo, useEffect } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { OpakeLogo } from "@/components/OpakeLogo"

type CallbackResult =
  | { state: "success"; errorMessage: "" }
  | { state: "error"; errorMessage: string }

function OAuthCallbackPage() {
  const { state, errorMessage } = useMemo<CallbackResult>(() => {
    const params = new URLSearchParams(window.location.search)
    const error = params.get("error")
    if (error) {
      return { state: "error", errorMessage: error }
    }
    return { state: "success", errorMessage: "" }
  }, [])

  useEffect(() => {
    // Strip query params from URL after reading them
    window.history.replaceState({}, "", window.location.pathname)
  }, [])

  return (
    <div className="bg-base-300 flex min-h-screen items-center justify-center font-sans">
      <div className="flex w-full max-w-md flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />

        {state === "success" && (
          <div className="flex flex-col items-center gap-6 text-center">
            <div className="bg-success/20 flex h-16 w-16 items-center justify-center rounded-full">
              <CheckIcon size={32} />
            </div>

            <div className="flex flex-col gap-2">
              <h1 className="text-base-content text-2xl font-semibold">CLI login successful</h1>
            </div>

            <div className="text-base-content/50 mt-2 flex flex-col gap-3 text-sm">
              <p>You can close this tab and return to your terminal.</p>
              <p>
                <span className="text-base-content/70 font-medium">Note:</span> This logs you into
                the CLI only. The web app requires a separate login.
              </p>
            </div>
          </div>
        )}

        {state === "error" && (
          <div className="flex flex-col items-center gap-6 text-center">
            <div className="bg-error/20 flex h-16 w-16 items-center justify-center rounded-full">
              <svg
                className="text-error h-8 w-8"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </div>

            <div className="flex flex-col gap-2">
              <h1 className="text-base-content text-2xl font-semibold">CLI login failed</h1>
              <p className="text-base-content/60">{errorMessage}</p>
            </div>

            <div className="text-base-content/50 mt-2 flex flex-col gap-3 text-sm">
              <p>Please try again from your terminal.</p>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export const Route = createFileRoute("/oauth/cli-callback")({
  component: OAuthCallbackPage,
})
