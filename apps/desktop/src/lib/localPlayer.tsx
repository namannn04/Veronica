/* The local music player, kept for the life of the window.
 *
 * Only the page on screen is mounted, and the player used to be an <audio>
 * element inside the Music page: going to any other page destroyed it, so the
 * music stopped, while the system media controls stayed registered and did
 * nothing. The player now lives here, above the pages, and the Music page is
 * one view onto it.
 */

import { convertFileSrc } from "@tauri-apps/api/core";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { ipc } from "./ipc";
import type { LocalTrack } from "./types";

type LocalPlayer = {
  tracks: LocalTrack[];
  /** Index into `tracks`, or -1 before anything has been chosen. */
  index: number;
  current: LocalTrack | undefined;
  playing: boolean;
  position: number;
  duration: number;
  play: (index: number) => void;
  toggle: () => void;
  next: () => void;
  previous: () => void;
  seek: (seconds: number) => void;
  /** Read ~/Music, once; the page calls this when it first opens. */
  loadLibrary: () => void;
};

const Context = createContext<LocalPlayer | null>(null);

export function LocalPlayerProvider({ children }: { children: ReactNode }) {
  // One element for the whole session; it never belongs to a page. Created
  // once, lazily, rather than a new one built and discarded every render.
  const [element] = useState(() => new Audio());
  const audio = useRef<HTMLAudioElement>(element);
  const [tracks, setTracks] = useState<LocalTrack[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [index, setIndex] = useState(-1);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const current = tracks[index];

  const loadLibrary = useCallback(() => {
    if (loaded) return;
    setLoaded(true);
    void ipc.musicLibrary().then(setTracks).catch(() => setTracks([]));
  }, [loaded]);

  const play = useCallback((next: number) => {
    const track = tracks[next];
    if (!track) return;
    const element = audio.current;
    setIndex(next);
    element.src = convertFileSrc(track.path);
    void element.play().catch(() => setPlaying(false));
  }, [tracks]);

  const toggle = useCallback(() => {
    const element = audio.current;
    if (!element.src) return;
    if (element.paused) void element.play().catch(() => setPlaying(false));
    else element.pause();
  }, []);

  const step = useCallback((by: number) => {
    if (tracks.length === 0) return;
    play(((index < 0 ? 0 : index) + by + tracks.length) % tracks.length);
  }, [index, play, tracks.length]);
  const next = useCallback(() => step(1), [step]);
  const previous = useCallback(() => step(-1), [step]);

  const seek = useCallback((seconds: number) => {
    if (Number.isFinite(seconds)) audio.current.currentTime = seconds;
  }, []);

  // The element's own events are the truth about playing and position.
  useEffect(() => {
    const element = audio.current;
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onTime = () => setPosition(element.currentTime);
    const onMeta = () => setDuration(Number.isFinite(element.duration) ? element.duration : 0);
    element.addEventListener("play", onPlay);
    element.addEventListener("pause", onPause);
    element.addEventListener("timeupdate", onTime);
    element.addEventListener("loadedmetadata", onMeta);
    return () => {
      element.removeEventListener("play", onPlay);
      element.removeEventListener("pause", onPause);
      element.removeEventListener("timeupdate", onTime);
      element.removeEventListener("loadedmetadata", onMeta);
    };
  }, []);

  // A track that ends moves on, as the page's player did.
  useEffect(() => {
    const element = audio.current;
    element.addEventListener("ended", next);
    return () => element.removeEventListener("ended", next);
  }, [next]);

  // The system media controls (keyboard keys, GNOME's media panel) follow the
  // player wherever the window is, rather than the page being open.
  useEffect(() => {
    if (!current || !("mediaSession" in navigator)) return;
    navigator.mediaSession.metadata = new MediaMetadata({
      title: current.title,
      artist: current.artist,
      album: current.album,
      artwork: current.artPath ? [{ src: convertFileSrc(current.artPath) }] : [],
    });
    navigator.mediaSession.setActionHandler("play", toggle);
    navigator.mediaSession.setActionHandler("pause", toggle);
    navigator.mediaSession.setActionHandler("nexttrack", next);
    navigator.mediaSession.setActionHandler("previoustrack", previous);
  }, [current, next, previous, toggle]);

  const value = useMemo<LocalPlayer>(() => ({
    tracks, index, current, playing, position, duration,
    play, toggle, next, previous, seek, loadLibrary,
  }), [tracks, index, current, playing, position, duration, play, toggle, next, previous, seek, loadLibrary]);

  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useLocalPlayer(): LocalPlayer {
  const player = useContext(Context);
  if (!player) throw new Error("useLocalPlayer needs a LocalPlayerProvider above it");
  return player;
}
