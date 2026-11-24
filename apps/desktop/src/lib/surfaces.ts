export function isEmojiPickerSurface(search: string): boolean {
  return new URLSearchParams(search).get("surface") === "emoji-picker";
}

export function isFrameProbeSurface(search: string): boolean {
  return new URLSearchParams(search).get("surface") === "frame-probe";
}
