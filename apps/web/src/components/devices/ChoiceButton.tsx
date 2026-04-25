import { cn } from "@/lib/cn";
import type { Icon } from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import type { ComponentProps } from "react";

const cardClass = cn(
  "card card-bordered bg-base-100 hover:border-primary/40 cursor-pointer p-5 text-left transition-colors w-46",
  "disabled:opacity-50 disabled:cursor-not-allowed",
);
type CommonProps = Readonly<{
  title: string;
  description: string;
  icon: Icon;
}>;

type LinkChoiceProps = CommonProps & { as: "Link" } & ComponentProps<typeof Link>;
type ButtonChoiceProps = CommonProps & {
  as: "Button";
} & React.ButtonHTMLAttributes<HTMLButtonElement>;

type Props = LinkChoiceProps | ButtonChoiceProps;

function CardContent({ icon: IconComponent, title, description }: CommonProps) {
  return (
    <>
      <IconComponent size={24} className="text-primary mb-3" aria-hidden="true" />
      <h2 className="text-base-content font-medium">{title}</h2>
      <p className="text-caption text-base-content/60 mt-1">{description}</p>
    </>
  );
}

export function ChoiceButton(props: Props) {
  const { as: as_, title, description, icon, className, ...rest } = props;
  const common = { title, description, icon };

  if (as_ === "Link") {
    return (
      <Link className={cn(cardClass, className)} {...(rest as ComponentProps<typeof Link>)}>
        <CardContent {...common} />
      </Link>
    );
  }

  return (
    <button
      className={cn(cardClass, className)}
      {...(rest as React.ButtonHTMLAttributes<HTMLButtonElement>)}
    >
      <CardContent {...common} />
    </button>
  );
}
