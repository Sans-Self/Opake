import type { Icon } from "@phosphor-icons/react";
import {
  LockIcon,
  GraphIcon,
  ShareNetworkIcon,
  GlobeIcon,
  SparkleIcon,
  BookOpenIcon,
  QuestionIcon,
  UsersThreeIcon,
  ArrowsLeftRightIcon,
} from "@phosphor-icons/react";
import { TerminalIcon } from "@phosphor-icons/react/dist/ssr";

export type IconName =
  | "lock"
  | "network"
  | "share"
  | "globe"
  | "sparkles"
  | "book"
  | "question"
  | "group"
  | "pairing"
  | "terminal";

const ICON_MAP: Readonly<Record<IconName, Icon>> = {
  lock: LockIcon,
  network: GraphIcon,
  share: ShareNetworkIcon,
  globe: GlobeIcon,
  sparkles: SparkleIcon,
  book: BookOpenIcon,
  question: QuestionIcon,
  group: UsersThreeIcon,
  pairing: ArrowsLeftRightIcon,
  terminal: TerminalIcon,
};

export function resolveIcon(name: string): Icon {
  const icon = ICON_MAP[name as IconName] as Icon | undefined;
  if (!icon) {
    throw new Error(
      `Unknown icon name: "${name}". Valid names: ${Object.keys(ICON_MAP).join(", ")}`,
    );
  }
  return icon;
}
