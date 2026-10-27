// Tinto's icons, from Lucide (lucide.dev), drawn in the text's colour. Text
// never relies on an emoji or a symbol character for meaning: it uses these.
// The names are Tinto's own, so a call site says what it means.

import {
  Bug,
  Check,
  ChevronDown,
  ChevronRight,
  Circle,
  Clock,
  Copy,
  Dot,
  Ellipsis,
  Flag,
  GitMerge,
  GitPullRequest,
  Image,
  Lock,
  Pencil,
  Play,
  Settings,
  Star,
  Ticket,
  TriangleAlert,
  X,
  type LucideIcon,
} from "lucide-react";

const ICONS = {
  flag: Flag,
  x: X,
  alert: TriangleAlert,
  clock: Clock,
  lock: Lock,
  ring: Circle,
  check: Check,
  more: Ellipsis,
  down: ChevronDown,
  right: ChevronRight,
  gear: Settings,
  play: Play,
  ticket: Ticket,
  pull: GitPullRequest,
  merge: GitMerge,
  photo: Image,
  bug: Bug,
  star: Star,
  "star-filled": Star,
  pencil: Pencil,
  copy: Copy,
  dot: Dot,
} satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof ICONS;

/** An icon in the text's colour. `className` sizes it where it sits; the
 *  default is `app-icon`. Decorative unless given a `label`. */
export function Icon({
  name,
  className = "app-icon",
  label,
}: {
  name: IconName;
  className?: string;
  label?: string;
}) {
  const Drawn = ICONS[name];
  return (
    <Drawn
      className={className}
      strokeWidth={1.75}
      fill={name === "star-filled" ? "currentColor" : "none"}
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? "img" : undefined}
      focusable="false"
    />
  );
}
