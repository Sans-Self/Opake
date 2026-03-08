import { OpakeLogo } from "@/components/OpakeLogo";

export function CheckingView() {
  return (
    <div className="flex flex-col items-center gap-4">
      <OpakeLogo loading size="2xl" />
      <p className="text-base-content/60 text-sm">Setting things up…</p>
    </div>
  );
}
