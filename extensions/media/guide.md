# Music

Music controls whatever is playing through MPRIS, the standard every Linux
player speaks: Spotify, a browser tab, VLC, Rhythmbox. It also plays the audio
files under `~/Music` right inside Veronica.

## Commands

- `vr media status` shows the current track, its player and position.
- `vr media players` lists every MPRIS player on the session bus.
- `vr media library [query]` lists local tracks, filtered by title, artist or
  album. `--limit` caps the list (40 by default).
- `vr media play`, `pause`, `toggle`, `next`, `previous` and `stop` control
  the active player and print what it is playing afterwards.

Every command accepts `--json`.

## On Home and in the notch

While the app runs, the now-playing card on Home and the notch activity follow
the active player. Presenter mode hides track names when "Track names" is
blurred.
