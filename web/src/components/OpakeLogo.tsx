import { OpakeLogoSquares, type LogoSize } from "./OpakeLogoSquares";

const TEXT_SIZES: Readonly<Record<LogoSize, string>> = {
  sm: "text-[0.9rem]",
  md: "text-[1.1rem]",
  lg: "text-[1.5rem]",
  xl: "text-[2rem]",
  "2xl": "text-[2.75rem]",
};

type Props = Readonly<{
  size?: LogoSize;
  loading?: boolean;
}>;

export function OpakeLogo({ size = "md", loading = false }: Props) {
  return (
    <div className="flex items-center gap-2.5">
      <OpakeLogoSquares size={size} loading={loading} />
      <span
        className={`font-display text-base-content font-medium tracking-[0.05em] ${TEXT_SIZES[size]}`}
      >
        Opake
      </span>
    </div>
  );
}
