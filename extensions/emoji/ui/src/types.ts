export interface EmojiGroup {
  id: string;
  name: string;
  symbol: string;
}

export interface EmojiRow {
  character: string;
  name: string;
  groupIndex: number;
  supportsSkinTones: boolean;
}

export interface EmojiCopyResult {
  character: string;
  copiedVia: string;
  inserted: boolean;
}
