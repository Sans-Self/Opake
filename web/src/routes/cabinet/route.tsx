import { useState, useEffect } from "react";
import { createFileRoute, redirect, Outlet } from "@tanstack/react-router";
import { Sidebar } from "@/components/cabinet/Sidebar";
import { TopBar } from "@/components/cabinet/TopBar";
import { useDocumentsStore } from "@/stores/documents";

function CabinetLayout() {
  const [searchQuery, setSearchQuery] = useState("");
  const fetchAll = useDocumentsStore((s) => s.fetchAll);

  useEffect(() => {
    void fetchAll();
  }, [fetchAll]);

  return (
    <div className="bg-base-300 flex h-screen overflow-hidden font-sans">
      <Sidebar />
      <main className="flex flex-1 flex-col overflow-hidden">
        <TopBar searchQuery={searchQuery} onSearchChange={setSearchQuery} />
        <Outlet />
      </main>
    </div>
  );
}

export const Route = createFileRoute("/cabinet")({
  beforeLoad: ({ context }) => {
    if (context.auth.session.status !== "active") {
      throw redirect({ to: "/devices/login" });
    }
  },
  component: CabinetLayout,
});
