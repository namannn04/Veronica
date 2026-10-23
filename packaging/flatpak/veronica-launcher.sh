#!/bin/sh
# Start Veronica or `vr` inside the Flatpak with the host's XDG directories.
#
# Flatpak points the XDG variables at ~/.var/app, but Veronica's settings,
# usage data and IPC socket are shared with its GNOME Shell extension, which
# runs on the host and reads ~/.config/veronica. The manifest grants the home
# directory, so the sandbox uses the same places the Debian package does.
export XDG_CONFIG_HOME="${HOST_XDG_CONFIG_HOME:-$HOME/.config}"
export XDG_DATA_HOME="${HOST_XDG_DATA_HOME:-$HOME/.local/share}"
export XDG_CACHE_HOME="${HOST_XDG_CACHE_HOME:-$HOME/.cache}"
export XDG_STATE_HOME="${HOST_XDG_STATE_HOME:-$HOME/.local/state}"
exec "/app/libexec/veronica/$(basename "$0")" "$@"
