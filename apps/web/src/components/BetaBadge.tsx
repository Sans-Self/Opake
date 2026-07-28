export function BetaBadge() {
  if (import.meta.env.VITE_BETA !== "1") return null;
  return (
    <p
      aria-label="Beta version"
      className="pointer-events-none fixed -right-13 top-8 z-50 rotate-45 select-none bg-amber-500 px-14 py-1.5 text-center text-base font-bold uppercase tracking-[0.3em] text-black shadow-lg"
    >
      Beta
    </p>
  );
}
