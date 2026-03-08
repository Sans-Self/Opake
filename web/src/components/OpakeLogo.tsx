import { useEffect, useRef } from "react";

const SIZES = {
  sm: { square: 16, wrap: 22, text: "text-[0.9rem]" },
  md: { square: 22, wrap: 28, text: "text-[1.1rem]" },
  lg: { square: 30, wrap: 36, text: "text-[1.5rem]" },
  xl: { square: 40, wrap: 48, text: "text-[2rem]" },
  "2xl": { square: 54, wrap: 64, text: "text-[2.75rem]" },
} as const;
type LogoSize = keyof typeof SIZES;

type Props = Readonly<{
  size?: LogoSize;
  loading?: boolean;
}>;

export function OpakeLogo({ size = "md", loading = false }: Props) {
  const { square, wrap, text } = SIZES[size];
  const SOLID_BG = "oklch(0.58 0.095 75 / 0.7)";
  const GHOST_BG = "oklch(0.58 0.095 75 / 0.2)";
  const GHOST_BORDER = "1.5px solid oklch(0.58 0.095 75 / 0.45)";
  const NO_BORDER = "0 solid transparent";

  const square1Style = {
    width: square,
    height: square,
    "--space": `${wrap - square}px`,
    "--sq-dir": 1,
    "--sq-from": SOLID_BG,
    "--sq-to": GHOST_BG,
    "--sq-border-from": NO_BORDER,
    "--sq-border-to": GHOST_BORDER,
    background: SOLID_BG,
    border: NO_BORDER,
  } as React.CSSProperties;

  const square2Style = {
    width: square,
    height: square,
    "--space": `${wrap - square}px`,
    "--sq-dir": -1,
    "--sq-from": GHOST_BG,
    "--sq-to": SOLID_BG,
    "--sq-border-from": GHOST_BORDER,
    "--sq-border-to": NO_BORDER,
    background: GHOST_BG,
    border: GHOST_BORDER,
  } as React.CSSProperties;

  const square1Ref = useRef<HTMLDivElement>(null);
  const square2Ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const els = [square1Ref.current, square2Ref.current].filter(
      (el): el is HTMLDivElement => el !== null,
    );
    if (els.length === 0) return;

    if (loading) {
      els.forEach((el) => el.classList.add("animate-logo-square"));
      return;
    }

    // loading stopped — let the current cycle finish, then remove
    const handleIteration = (e: AnimationEvent) => {
      const el = e.currentTarget as HTMLElement;
      el.classList.remove("animate-logo-square");
      el.removeEventListener("animationiteration", handleIteration as EventListener);
    };

    els.forEach((el) => {
      if (!el.classList.contains("animate-logo-square")) return;
      el.addEventListener("animationiteration", handleIteration as EventListener);
    });

    return () => {
      els.forEach((el) =>
        el.removeEventListener("animationiteration", handleIteration as EventListener),
      );
    };
  }, [loading]);

  return (
    <div className="flex items-center gap-2.5">
      <div className="relative shrink-0" style={{ width: wrap, height: wrap }}>
        <div className="absolute top-0 left-0 rounded-sm" ref={square1Ref} style={square1Style} />
        <div
          className="absolute right-0 bottom-0 rounded-sm"
          ref={square2Ref}
          style={square2Style}
        />
      </div>
      <span className={`font-display text-base-content font-medium tracking-[0.05em] ${text}`}>
        Opake
      </span>
    </div>
  );
}
