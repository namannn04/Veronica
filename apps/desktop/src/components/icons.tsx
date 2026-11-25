import {
  AppWindow,
  BellRing,
  Blocks,
  CalendarDays,
  ClipboardList,
  Cpu,
  Database,
  Gauge,
  House,
  Info,
  type LucideIcon,
  type LucideProps,
  Music2,
  Network,
  NotebookPen,
  Palette,
  PanelsTopLeft,
  Plane,
  ScanSearch,
  Search,
  Server,
  Settings,
  Smile,
  Target,
  Wrench,
} from "lucide-react";
import type { ReactNode } from "react";

const ICON_SIZE = 16;
const ICON_STROKE = 1.75;

function icon(Glyph: LucideIcon) {
  return function Icon(props: LucideProps) {
    return (
      <Glyph
        size={ICON_SIZE}
        strokeWidth={ICON_STROKE}
        aria-hidden="true"
        focusable="false"
        {...props}
      />
    );
  };
}

export const HomeIcon = icon(House);
export const UsageIcon = icon(Gauge);
export const AlertsIcon = icon(BellRing);
export const HerdrIcon = icon(PanelsTopLeft);
export const QuinjetIcon = icon(Plane);
export const WorkspaceIcon = icon(AppWindow);
export const MusicIcon = icon(Music2);
export const CalendarIcon = icon(CalendarDays);
export const AttentionIcon = icon(Target);
export const SystemIcon = icon(Cpu);
export const MachinesIcon = icon(Server);
export const MaintenanceIcon = icon(Wrench);
export const DatabaseIcon = icon(Database);
export const ClipboardIcon = icon(ClipboardList);
export const ColorIcon = icon(Palette);
export const EmojiIcon = icon(Smile);
export const AuditIcon = icon(ScanSearch);
export const CompanionIcon = icon(NotebookPen);
export const ExtensionsIcon = icon(Blocks);
export const SettingsIcon = icon(Settings);
export const AboutIcon = icon(Info);
export const PortsIcon = icon(Network);
export const SearchIcon = icon(Search);

const NAMED_ICONS: Record<string, (props: LucideProps) => ReactNode> = {
  hourglass: AttentionIcon,
  "columns-3": HerdrIcon,
  "git-branch": QuinjetIcon,
  bell: AlertsIcon,
  gauge: UsageIcon,
  music: MusicIcon,
  "scan-search": AuditIcon,
  calendar: CalendarIcon,
  pipette: ColorIcon,
  smile: EmojiIcon,
};

export function NamedIcon({ name }: { name?: string }) {
  const Glyph = (name === undefined ? undefined : NAMED_ICONS[name]) ?? ExtensionsIcon;
  return <Glyph />;
}
