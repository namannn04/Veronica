/** One request at a time; hidden webviews do no polling and resume immediately. */
export function startVisiblePolling(task: () => Promise<unknown>, intervalMs: number, surface: Pick<Document, "hidden" | "addEventListener" | "removeEventListener"> = document, immediate = true) {
  let live = true, busy = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const clear = () => { if (timer !== undefined) clearTimeout(timer); timer = undefined; };
  const schedule = () => { clear(); if (live && !surface.hidden) timer = setTimeout(() => { void tick(); }, intervalMs); };
  const tick = async () => {
    clear();
    if (!live || surface.hidden || busy) return;
    busy = true;
    try { await task(); } catch { /* The caller owns the visible error state. */ }
    finally { busy = false; schedule(); }
  };
  const visibility = () => { clear(); if (!surface.hidden) void tick(); };
  surface.addEventListener("visibilitychange", visibility);
  if (immediate) void tick(); else schedule();
  return () => { live = false; clear(); surface.removeEventListener("visibilitychange", visibility); };
}
