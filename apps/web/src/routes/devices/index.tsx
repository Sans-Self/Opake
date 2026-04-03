import { createFileRoute, redirect } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { FreshAccountView } from "@/components/devices/FreshAccountView";
// import { RecoverIdentityView } from "@/components/devices/RecoverIdentityView";
// import { ConflictView } from "@/components/devices/ConflictView";
// import { ReadyView } from "@/components/devices/ReadyView";

export const Route = createFileRoute("/devices/")({
  beforeLoad: async () => {
    const state = useAuthStore.getState();
    if (state.session.status === "none") {
      await state.boot();
    }

    if (useAuthStore.getState().session.status !== "active") {
      throw redirect({ to: "/devices/login" });
    }
  },
  component: DevicesPage,
});

function DevicesPage() {
  const identity = useAuthStore((s) => s.identity);

  // useEffect(() => {
  //   if (childRouteActive) return;
  //   if (identity.status !== "unchecked") return;
  //   void useAuthStore.getState().checkIdentity();
  // }, [identity.status, childRouteActive]);
  // if (childRouteActive) return null;
  // const isChecking = identity.status === "unchecked" || identity.status === "checking";
  // if (isChecking || !minTimeElapsed) {
  //   return <CheckingView />;
  // }
  console.log(identity.status);
  // eslint-disable-next-line sonarjs/no-small-switch
  switch (identity.status) {
    case "fresh":
      return <FreshAccountView />;
    // case "remote_only":
    //   return <RecoverIdentityView />;
    // case "conflict":
    //   return <ConflictView />;
    // case "ready":
    //   return <ReadyView />;
  }

  return <span>hi</span>;
}
