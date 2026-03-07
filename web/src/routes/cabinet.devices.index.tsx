import { createFileRoute, redirect, Link } from "@tanstack/react-router"
import { OpakeLogo } from "@/components/OpakeLogo"
import { useAuthStore } from "@/stores/auth"
import { ArrowsLeftRightIcon, KeyIcon } from "@phosphor-icons/react"

function DevicesPage() {
  const phase = useAuthStore((s) => s.phase)

  if (phase === "ready") {
    return <ActiveIdentityView />
  }

  return <IdentityRequiredView />
}

/** UserIcon already has identity on this device — show device management. */
function ActiveIdentityView() {
  return (
    <div className="bg-base-300 flex min-h-screen items-center justify-center font-sans">
      <div className="flex w-full max-w-lg flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />

        <div className="text-center">
          <h1 className="text-base-content text-2xl font-semibold">Device identity active</h1>
          <p className="text-base-content/60 mt-2 text-sm">
            This device has an encryption identity. You can approve pairing requests from other
            devices.
          </p>
        </div>

        <Link to="/cabinet/devices/pair" className="btn btn-neutral w-full max-w-xs">
          <ArrowsLeftRightIcon size={20} aria-hidden="true" />
          Manage pairing
        </Link>

        <Link to="/cabinet" className="text-base-content/50 hover:text-base-content/70 text-sm">
          Back to cabinet
        </Link>
      </div>
    </div>
  )
}

/** UserIcon needs to get their identity onto this device. */
function IdentityRequiredView() {
  return (
    <div className="bg-base-300 flex min-h-screen items-center justify-center font-sans">
      <div className="flex w-full max-w-lg flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />

        <div className="text-center">
          <h1 className="text-base-content text-2xl font-semibold">Set up encryption</h1>
          <p className="text-base-content/60 mt-2 text-sm">
            Your account has an encryption identity on another device. Transfer it to use Opake
            here.
          </p>
        </div>

        <div className="grid w-full max-w-md gap-4 sm:grid-cols-2">
          <Link
            to="/cabinet/devices/pair"
            className="card card-bordered bg-base-100 hover:border-primary/40 p-5 transition-colors"
          >
            <ArrowsLeftRightIcon size={24} className="text-primary mb-3" aria-hidden="true" />
            <h2 className="text-base-content font-medium">Pair with existing device</h2>
            <p className="text-caption text-base-content/60 mt-1">
              Transfer your encryption identity from another device.
            </p>
          </Link>

          <div className="card card-bordered bg-base-100 p-5 opacity-50" aria-disabled="true">
            <KeyIcon size={24} className="text-base-content/40 mb-3" aria-hidden="true" />
            <h2 className="text-base-content font-medium">Recover from seed phrase</h2>
            <p className="text-caption text-base-content/60 mt-1">
              Restore your identity from your backup phrase.
            </p>
            <span className="text-base-content/40 mt-2 inline-block text-xs">Coming soon</span>
          </div>
        </div>
      </div>
    </div>
  )
}

export const Route = createFileRoute("/cabinet/devices/")({
  beforeLoad: () => {
    const state = useAuthStore.getState()
    if (state.phase !== "ready" && state.phase !== "awaiting_identity") {
      throw redirect({ to: "/login" })
    }
  },
  component: DevicesPage,
})
