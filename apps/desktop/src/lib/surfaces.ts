export function isEmojiPickerSurface(search: string): boolean {
  return new URLSearchParams(search).get("surface") === "emoji-picker";
}
