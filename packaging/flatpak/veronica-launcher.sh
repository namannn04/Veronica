#!/bin/sh
export XDG_CONFIG_HOME="${HOST_XDG_CONFIG_HOME:-$HOME/.config}"
export XDG_DATA_HOME="${HOST_XDG_DATA_HOME:-$HOME/.local/share}"
export XDG_CACHE_HOME="${HOST_XDG_CACHE_HOME:-$HOME/.cache}"
export XDG_STATE_HOME="${HOST_XDG_STATE_HOME:-$HOME/.local/state}"
exec "/app/libexec/veronica/$(basename "$0")" "$@"
