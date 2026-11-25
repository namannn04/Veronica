import type { Client } from "@veronica/extension-api";
import { useClient } from "@veronica/ui/frame";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { LocalTrack } from "./types";

export type LibraryStatus = "idle" | "loading" | "ready" | "failed";

type LocalPlayer = {
  tracks: LocalTrack[];
  libraryStatus: LibraryStatus;
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
  loadLibrary: () => void;
};

const Context = createContext<LocalPlayer | null>(null);

export function useFileUrl(path: string | null | undefined): string | null {
  const client = useClient();
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    setUrl(null);
    if (!path) {
      return;
    }
    let live = true;
    void client.files
      .url(path)
      .then((resolved) => {
        if (live) {
          setUrl(resolved);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [client, path]);
  return url;
}

async function playableSource(client: Client, path: string) {
  const response = await fetch(await client.files.url(path));
  if (!response.ok) {
    throw new Error(`could not read ${path}`);
  }
  return URL.createObjectURL(await response.blob());
}

export function LocalPlayerProvider({ children }: { children: ReactNode }) {
  const client = useClient();
  const [element] = useState(() => new Audio());
  const audio = useRef<HTMLAudioElement>(element);
  const [tracks, setTracks] = useState<LocalTrack[]>([]);
  const [libraryStatus, setLibraryStatus] = useState<LibraryStatus>("idle");
  const [index, setIndex] = useState(-1);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const current = tracks[index];

  const loadLibrary = useCallback(() => {
    if (libraryStatus === "loading" || libraryStatus === "ready") {
      return;
    }
    setLibraryStatus("loading");
    void client.backend
      .call<LocalTrack[]>("library")
      .then((list) => {
        setTracks(list);
        setLibraryStatus("ready");
      })
      .catch(() => {
        setTracks([]);
        setLibraryStatus("failed");
      });
  }, [client, libraryStatus]);

  const play = useCallback(
    (next: number) => {
      const track = tracks[next];
      if (!track) {
        return;
      }
      const element = audio.current;
      setIndex(next);
      void playableSource(client, track.path)
        .then((source) => {
          if (element.src.startsWith("blob:")) {
            URL.revokeObjectURL(element.src);
          }
          element.src = source;
          return element.play();
        })
        .catch(() => setPlaying(false));
    },
    [client, tracks],
  );

  const toggle = useCallback(() => {
    const element = audio.current;
    if (!element.src) {
      return;
    }
    if (element.paused) {
      void element.play().catch(() => setPlaying(false));
    } else {
      element.pause();
    }
  }, []);

  const step = useCallback(
    (by: number) => {
      if (tracks.length === 0) {
        return;
      }
      play((Math.max(index, 0) + by + tracks.length) % tracks.length);
    },
    [index, play, tracks.length],
  );
  const next = useCallback(() => step(1), [step]);
  const previous = useCallback(() => step(-1), [step]);

  const seek = useCallback((seconds: number) => {
    if (Number.isFinite(seconds)) {
      audio.current.currentTime = seconds;
    }
  }, []);
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
  useEffect(() => {
    const element = audio.current;
    element.addEventListener("ended", next);
    return () => element.removeEventListener("ended", next);
  }, [next]);
  const artwork = useFileUrl(current?.artPath);
  useEffect(() => {
    if (!current || !("mediaSession" in navigator)) {
      return;
    }
    navigator.mediaSession.metadata = new MediaMetadata({
      title: current.title,
      artist: current.artist,
      album: current.album,
      artwork: artwork ? [{ src: artwork }] : [],
    });
    navigator.mediaSession.setActionHandler("play", toggle);
    navigator.mediaSession.setActionHandler("pause", toggle);
    navigator.mediaSession.setActionHandler("nexttrack", next);
    navigator.mediaSession.setActionHandler("previoustrack", previous);
  }, [artwork, current, next, previous, toggle]);

  const value = useMemo<LocalPlayer>(
    () => ({
      tracks,
      libraryStatus,
      index,
      current,
      playing,
      position,
      duration,
      play,
      toggle,
      next,
      previous,
      seek,
      loadLibrary,
    }),
    [
      tracks,
      libraryStatus,
      index,
      current,
      playing,
      position,
      duration,
      play,
      toggle,
      next,
      previous,
      seek,
      loadLibrary,
    ],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useLocalPlayer(): LocalPlayer {
  const player = useContext(Context);
  if (!player) {
    throw new Error("useLocalPlayer needs a LocalPlayerProvider above it");
  }
  return player;
}
