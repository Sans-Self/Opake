import type { Icon } from "@phosphor-icons/react";
import {
  LockIcon,
  GraphIcon,
  ShareNetworkIcon,
  GlobeIcon,
  SparkleIcon,
  EyeSlashIcon,
  BookOpenIcon,
  QuestionIcon,
  UsersThreeIcon,
  ArrowsLeftRightIcon,
  PlantIcon,
  FolderIcon,
  LightningIcon,
  AtomIcon,
} from "@phosphor-icons/react";
import { TerminalIcon } from "@phosphor-icons/react/dist/ssr";

export type IconName =
  | "lock"
  | "network"
  | "share"
  | "globe"
  | "sparkles"
  | "eye-slash"
  | "book"
  | "question"
  | "group"
  | "pairing"
  | "seedling"
  | "terminal"
  | "folder"
  | "lightning"
  | "react";

const ICON_MAP: Readonly<Record<IconName, Icon>> = {
  lock: LockIcon,
  network: GraphIcon,
  share: ShareNetworkIcon,
  globe: GlobeIcon,
  sparkles: SparkleIcon,
  "eye-slash": EyeSlashIcon,
  book: BookOpenIcon,
  question: QuestionIcon,
  group: UsersThreeIcon,
  pairing: ArrowsLeftRightIcon,
  seedling: PlantIcon,
  terminal: TerminalIcon,
  folder: FolderIcon,
  lightning: LightningIcon,
  react: AtomIcon,
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
