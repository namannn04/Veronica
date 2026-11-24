import {
  AppWindow,
  Blocks,
  CalendarDays,
  ClipboardList,
  Cpu,
  Database,
  Gauge,
  House,
  Info,
  Music2,
  NotebookPen,
  Palette,
  PanelsTopLeft,
  Plane,
  ScanSearch,
  Network,
  Search,
  Server,
  Settings,
  Smile,
  Target,
  Wrench,
  type LucideIcon,
  type LucideProps,
} from "lucide-react";

const ICON_SIZE = 16;
const ICON_STROKE = 1.75;

function icon(Glyph: LucideIcon) {
  return function Icon(props: LucideProps) {
    return <Glyph size={ICON_SIZE} strokeWidth={ICON_STROKE} aria-hidden="true" focusable="false" {...props} />;
  };
}

export const HomeIcon = icon(House);
export const UsageIcon = icon(Gauge);
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
