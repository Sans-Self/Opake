const SIZES = {
  sm: { square: 16, wrap: 22, text: "text-[0.9rem]" },
  md: { square: 22, wrap: 28, text: "text-[1.1rem]" },
  lg: { square: 30, wrap: 36, text: "text-[1.5rem]" },
} as const;

type LogoSize = keyof typeof SIZES;

export function OpakeLogo({ size = "md" }: { size?: LogoSize }) {
  const { square, wrap, text } = SIZES[size];

  return (
    <div className="flex items-center gap-2.5">
      <div
        className="relative shrink-0"
        style={{ width: wrap, height: wrap }}
      >
        <div
          className="absolute top-0 left-0 rounded-[3px] bg-primary/70"
          style={{ width: square, height: square }}
        />
        <div
          className="absolute right-0 bottom-0 rounded-[3px] border-[1.5px] border-primary/45 bg-primary/20"
          style={{ width: square, height: square }}
        />
      </div>
      <span
        className={`font-display font-medium tracking-[0.05em] text-base-content ${text}`}
      >
        Opake
      </span>
    </div>
  );
}
