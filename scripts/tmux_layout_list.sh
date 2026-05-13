#!/usr/bin/env bash

[ -z "${DEBUG:-}" ] || set -x

set -euo pipefail

# List help function
#
# Displays help information for the list command.
_show_list_help() {
	cat <<EOF
tmux-layout list - list available layouts

USAGE:
    tmux-layout list

DESCRIPTION:
    Prints the names of every *.yml and *.yaml file in
    \${XDG_CONFIG_HOME:-\$HOME/.config}/tmux/layouts/.
EOF
}

# Main list command implementation
#
# Prints layout basenames (without extension) from the layout directory.
_tmux_layout_list() {
	case "${1:-}" in
	--help | -h | help)
		_show_list_help
		return 0
		;;
	"") ;;
	*)
		echo "tmux-layout list: unexpected argument '$1'" >&2
		return 1
		;;
	esac

	local layout_dir="${XDG_CONFIG_HOME:-$HOME/.config}/tmux/layouts"

	if [ ! -d "$layout_dir" ]; then
		echo "tmux-layout: layout directory does not exist: $layout_dir" >&2
		return 1
	fi

	shopt -s nullglob
	local f any=0
	for f in "$layout_dir"/*.yml "$layout_dir"/*.yaml; do
		basename "$f" | sed -E 's/\.(yml|yaml)$//'
		any=1
	done
	shopt -u nullglob

	if ((any == 0)); then
		echo "tmux-layout: no layouts in $layout_dir" >&2
		return 1
	fi
}
