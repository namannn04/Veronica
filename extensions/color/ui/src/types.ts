export interface SwatchRow {
  id: number;
  hex: string;
  red: number;
  green: number;
  blue: number;
  profile: "srgb" | "displayP3";
  profileLabel: string;
  pickedAt: string;
  formats: Record<string, string>;
  prefersDarkText: boolean;
}

export interface CopyFormatOption {
  id: string;
  label: string;
}

export interface PickResult {
  swatch: SwatchRow;
  value: string;
  format: string;
  source: string;
  copiedVia: string | null;
  copyError: string | null;
}

export interface CopyResult {
  value: string;
  format: string;
  copiedVia: string;
}
