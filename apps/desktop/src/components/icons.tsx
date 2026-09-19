/* Veronica's icon set.
 *
 * These replace the Unicode glyphs the sidebar used to draw. A character like
 * `⌁` or `⑂` is at the mercy of whichever font on the machine happens to
 * contain it, so the old rail mixed weights, sizes and baselines down its
 * length. Drawn paths are the same weight everywhere and inherit `currentColor`,
 * which is what lets one icon read correctly on all thirteen themes.
 *
 * One grid, one stroke: 24x24, 1.7px, round caps and joins, no fills.
 */

import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement>;

function Icon({ children, ...props }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...props}
    >
      {children}
    </svg>
  );
}

export const HomeIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M3.5 10.5 12 3.5l8.5 7" />
    <path d="M5.5 9.5v10h13v-10" />
    <path d="M9.5 19.5v-6h5v6" />
  </Icon>
);

export const UsageIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M12 3.5a8.5 8.5 0 0 1 8.5 8.5" />
    <path d="M12 7.5v4.5l3 2" />
  </Icon>
);

export const HerdrIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    <path d="M3.5 9h17" />
    <path d="M10 9v10.5" />
  </Icon>
);

export const QuinjetIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="7" cy="6" r="2.5" />
    <circle cx="7" cy="18" r="2.5" />
    <circle cx="17" cy="12" r="2.5" />
    <path d="M7 8.5v7" />
    <path d="M9.5 6.6c3 .6 4.4 2.2 5.1 4.2" />
    <path d="M14.6 13.2c-.7 2-2.1 3.6-5.1 4.2" />
  </Icon>
);

export const MusicIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M9 18V6.2l10-2v11.6" />
    <circle cx="6.5" cy="18" r="2.5" />
    <circle cx="16.5" cy="15.8" r="2.5" />
  </Icon>
);

export const CalendarIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="3.5" y="5" width="17" height="15" rx="2.5" />
    <path d="M3.5 10h17" />
    <path d="M8 3.5v3M16 3.5v3" />
    <path d="M7.5 14h3" />
  </Icon>
);

export const AttentionIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="12" cy="12" r="8.5" />
    <circle cx="12" cy="12" r="4" />
    <circle cx="12" cy="12" r=".6" fill="currentColor" />
  </Icon>
);

export const SystemIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="7" y="7" width="10" height="10" rx="2" />
    <path d="M10 3.5v3.5M14 3.5v3.5M10 17v3.5M14 17v3.5" />
    <path d="M3.5 10H7M3.5 14H7M17 10h3.5M17 14h3.5" />
  </Icon>
);

export const MachinesIcon = (props: IconProps) => (
  <Icon {...props}>
    <rect x="3.5" y="4.5" width="17" height="6" rx="1.8" />
    <rect x="3.5" y="13.5" width="17" height="6" rx="1.8" />
    <path d="M7 7.5h.01M7 16.5h.01" />
  </Icon>
);

export const MaintenanceIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M12 3.2 20 7.6v8.8L12 20.8 4 16.4V7.6z" />
    <path d="M12 12v8.8" />
    <path d="m4 7.6 8 4.4 8-4.4" />
  </Icon>
);

export const DatabaseIcon = (props: IconProps) => (
  <Icon {...props}>
    <ellipse cx="12" cy="6.5" rx="7.5" ry="3" />
    <path d="M4.5 6.5v11c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-11" />
    <path d="M4.5 12c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3" />
  </Icon>
);

export const ClipboardIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M9 4.5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-12a2 2 0 0 0-2-2h-2" />
    <rect x="9" y="2.8" width="6" height="3.6" rx="1.2" />
    <path d="M8.5 12h7M8.5 15.5h4.5" />
  </Icon>
);

export const ColorIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.2 0 1.8-.8 1.8-1.7 0-1.2-1-1.6-1-2.6 0-.8.7-1.4 1.6-1.4h1.4a4.7 4.7 0 0 0 4.7-4.7c0-3.7-3.8-6.6-8.5-6.6Z" />
    <circle cx="8" cy="10" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="12" cy="7.6" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="16" cy="10" r="1.1" fill="currentColor" stroke="none" />
  </Icon>
);

export const EmojiIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M8.5 14c.8 1.4 2 2.2 3.5 2.2s2.7-.8 3.5-2.2" />
    <path d="M9 9.5h.01M15 9.5h.01" />
  </Icon>
);

export const AuditIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="10.5" cy="10.5" r="6.5" />
    <path d="m15.2 15.2 5.3 5.3" />
    <path d="M7.8 10.6l1.9 1.9 3.5-3.7" />
  </Icon>
);

export const CompanionIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M12 3.2 13.9 9l6.1.2-4.9 3.6 1.8 5.9-4.9-3.4-4.9 3.4 1.8-5.9L4 9.2 10.1 9z" />
  </Icon>
);

export const ExtensionsIcon = (props: IconProps) => (
  <Icon {...props}>
    <path d="M9.5 4.5h5v2.2a2 2 0 1 0 2.8 2.8h2.2v5h-2.2a2 2 0 1 0-2.8 2.8v2.2h-5v-2.2a2 2 0 1 0-2.8-2.8H4.5v-5h2.2a2 2 0 1 0 2.8-2.8z" />
  </Icon>
);

export const SettingsIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="12" cy="12" r="3.1" />
    <path d="M19.2 14.6a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5v.2a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-1-1.5 1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1h-.2a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.5-1 1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3h.1a1.6 1.6 0 0 0 1-1.5v-.2a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8v.1a1.6 1.6 0 0 0 1.5 1h.2a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z" />
  </Icon>
);

export const AboutIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="12" cy="12" r="8.5" />
    <path d="M12 11v5.2" />
    <path d="M12 7.9h.01" />
  </Icon>
);

export const SearchIcon = (props: IconProps) => (
  <Icon {...props}>
    <circle cx="11" cy="11" r="6.5" />
    <path d="m15.6 15.6 4.9 4.9" />
  </Icon>
);
