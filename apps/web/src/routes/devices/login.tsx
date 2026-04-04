import { createFileRoute, redirect } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";

export const Route = createFileRoute("/devices/login")({
  beforeLoad: async () => {
    const state = useAuthStore.getState();
    if (state.session.status === "initializing") {
      await state.boot();
    }
    if (useAuthStore.getState().session.status === "active") {
      throw redirect({ to: "/devices" });
    }
  },
});
