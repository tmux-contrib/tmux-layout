#!/usr/bin/env bash

[ -z "${DEBUG:-}" ] || set -x

set -euo pipefail

readonly _TMUX_LAYOUT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/tmux/layouts"

# Verify the tools the switch command needs are installed
_check_switch_dependencies() {
	local missing=()

	if ! command -v tmux &>/dev/null; then
		missing+=("tmux     — https://github.com/tmux/tmux")
	fi
	if ! command -v yq &>/dev/null; then
		missing+=("yq       — https://github.com/mikefarah/yq (Go)")
	elif ! yq --version 2>&1 | grep -qi mikefarah; then
		echo "tmux-layout: yq must be mikefarah/yq (Go); a different 'yq' was found on PATH" >&2
		return 1
	fi
	if ! command -v envsubst &>/dev/null; then
		missing+=("envsubst — from gettext (macOS: brew install gettext)")
	fi

	if [[ ${#missing[@]} -gt 0 ]]; then
		echo "tmux-layout: missing required dependencies:" >&2
		for dep in "${missing[@]}"; do
			echo "  $dep" >&2
		done
		return 1
	fi
}

# Switch help function
#
# Displays help information for the switch command.
_show_switch_help() {
	cat <<EOF
tmux-layout switch - apply a YAML-defined tmux layout

USAGE:
    tmux-layout switch <name>

DESCRIPTION:
    Loads ${_TMUX_LAYOUT_DIR}/<name>.yml and applies it.

    Inside tmux:  the layout's windows are appended to the current session.
    Outside tmux: a new session named session.name is created and attached.
                  If the session already exists, attach without modifying.

    If \$IN_NIX_SHELL is set, every pane command is prefixed with
    'nix develop -c' so tools from the dev shell remain available.

    \${VAR} references in the YAML are expanded via envsubst before parsing.

    An optional 'cwd:' field may be set on session, window, or pane.
    Precedence is pane > window > session. A leading '~' expands to \$HOME;
    \${VAR} forms expand via envsubst. Anything else is passed to tmux -c
    as-is (absolute or relative to the directory tmux is invoked from).

EXAMPLES:
    tmux-layout switch dev
    tmux-layout switch \${PROJECT}
EOF
}

# Resolve a layout name to an absolute path
#
# Tries <name>.yml then <name>.yaml under the layout directory.
# Prints the resolved path or exits with an error.
_resolve_layout() {
	local name=$1 path
	for path in "$_TMUX_LAYOUT_DIR/$name.yml" "$_TMUX_LAYOUT_DIR/$name.yaml"; do
		if [ -f "$path" ]; then
			printf '%s\n' "$path"
			return 0
		fi
	done
	echo "tmux-layout: layout '$name' not found in $_TMUX_LAYOUT_DIR" >&2
	return 1
}

# Read a string field from the rendered YAML
#
# Returns an empty string for null/missing values.
# Reads from the _tmux_layout_yaml global.
#
# Usage: _yaml_get <yq-expression>
_yaml_get() {
	yq -r "$1 // \"\"" <<<"$_tmux_layout_yaml"
}

# Read an integer field from the rendered YAML
#
# Usage: _yaml_get_int <yq-expression>
_yaml_get_int() {
	yq -r "$1" <<<"$_tmux_layout_yaml"
}

# Expand a leading ~ in a path. $VAR forms are already expanded by envsubst.
#
# `~user` is intentionally not supported — write the absolute path instead.
#
# Usage: _expand_cwd <path>
_expand_cwd() {
	local p=$1
	[ -z "$p" ] && return 0
	# shellcheck disable=SC2088 # ~ inside the case pattern is a literal match, not tilde expansion
	case $p in
	'~')   printf '%s' "$HOME" ;;
	'~/'*) printf '%s%s' "$HOME" "${p#'~'}" ;;
	*)     printf '%s' "$p" ;;
	esac
}

# Resolve the effective cwd for a pane.
#
# Precedence: pane > window > session. Echoes empty if none set.
#
# Usage: _resolve_cwd <window-index> <pane-index>
_resolve_cwd() {
	local i=$1 j=$2 v
	v=$(_yaml_get ".windows[$i].panes[$j].cwd")
	[ -z "$v" ] && v=$(_yaml_get ".windows[$i].cwd")
	[ -z "$v" ] && v=$(_yaml_get '.session.cwd')
	_expand_cwd "$v"
}

# Build the shell-command string for tmux to spawn
#
# Returns an empty string for empty input (keeps the pane in the default shell).
# Prefixes with 'nix develop -c' when IN_NIX_SHELL is set.
#
# Usage: _build_pane_cmd <command-string>
_build_pane_cmd() {
	local cmd=$1
	[ -z "$cmd" ] && return 0
	if [ -n "${IN_NIX_SHELL:-}" ]; then
		printf 'nix develop -c %q -c %q' "${SHELL:-bash}" "$cmd"
	else
		printf '%s' "$cmd"
	fi
}

# Create a new window in the given session
#
# Echoes the new window id.
#
# Usage: _new_window <target-session> <window-name> <cwd> <cmd-string>
_new_window() {
	local target=$1 wname=$2 cwd=$3 cmd=$4
	local -a args=(-d -P -F '#{window_id}' -t "$target")
	[ -n "$wname" ] && args+=(-n "$wname")
	[ -n "$cwd" ]   && args+=(-c "$cwd")
	if [ -n "$cmd" ]; then
		tmux new-window "${args[@]}" "$cmd"
	else
		tmux new-window "${args[@]}"
	fi
}

# Split a window and echo the new pane id
#
# Usage: _split_window <window-id> <cwd> <cmd-string>
_split_window() {
	local wid=$1 cwd=$2 cmd=$3
	local -a args=(-t "$wid" -P -F '#{pane_id}')
	[ -n "$cwd" ] && args+=(-c "$cwd")
	if [ -n "$cmd" ]; then
		tmux split-window "${args[@]}" "$cmd"
	else
		tmux split-window "${args[@]}"
	fi
}

# Add panes 1..N to a window
#
# The first pane is created with the window itself; this populates the rest.
#
# Usage: _add_panes_after_first <window-index> <window-id>
_add_panes_after_first() {
	local i=$1 wid=$2 p_count
	p_count=$(_yaml_get_int ".windows[$i].panes | length")
	local j p_name p_cmd p_cwd p_string pane_id
	for ((j = 1; j < p_count; j++)); do
		p_name=$(_yaml_get   ".windows[$i].panes[$j].name")
		p_cmd=$(_yaml_get    ".windows[$i].panes[$j].command")
		p_cwd=$(_resolve_cwd "$i" "$j")
		p_string=$(_build_pane_cmd "$p_cmd")
		pane_id=$(_split_window "$wid" "$p_cwd" "$p_string")
		if [ -n "$p_name" ]; then
			tmux select-pane -t "$pane_id" -T "$p_name"
		fi
	done
}

# Apply the optional `layout:` field to a window via `tmux select-layout`
#
# Usage: _apply_window_layout <window-index> <window-id>
_apply_window_layout() {
	local i=$1 wid=$2 wlayout
	wlayout=$(_yaml_get ".windows[$i].layout")
	[ -n "$wlayout" ] && tmux select-layout -t "$wid" "$wlayout"
	return 0
}

# Create a complete window (first pane + remaining panes + layout)
#
# Usage: _create_window_full <window-index> <target-session>
_create_window_full() {
	local i=$1 target=$2
	local w_name p_count p0_name p0_cmd p0_cwd p0_string wid
	w_name=$(_yaml_get ".windows[$i].name")
	p_count=$(_yaml_get_int ".windows[$i].panes | length")
	p0_name=""
	p0_cmd=""
	if ((p_count > 0)); then
		p0_name=$(_yaml_get ".windows[$i].panes[0].name")
		p0_cmd=$(_yaml_get  ".windows[$i].panes[0].command")
	fi
	p0_cwd=$(_resolve_cwd "$i" 0)
	p0_string=$(_build_pane_cmd "$p0_cmd")
	wid=$(_new_window "$target" "$w_name" "$p0_cwd" "$p0_string")
	[ -n "$p0_name" ] && tmux select-pane -t "$wid" -T "$p0_name"
	_add_panes_after_first "$i" "$wid"
	_apply_window_layout "$i" "$wid"
}

# Main switch command implementation
#
# Loads, env-substitutes, and applies a layout file.
#
# Usage: _tmux_layout_switch <name>
_tmux_layout_switch() {
	case "${1:-}" in
	--help | -h | help)
		_show_switch_help
		return 0
		;;
	"")
		echo "tmux-layout switch: missing layout name" >&2
		echo "Run 'tmux-layout switch --help' for usage." >&2
		return 1
		;;
	-*)
		echo "tmux-layout switch: unknown flag '$1'" >&2
		return 1
		;;
	esac

	if [[ $# -gt 1 ]]; then
		echo "tmux-layout switch: too many arguments" >&2
		return 1
	fi

	_check_switch_dependencies || return 1

	local path
	path=$(_resolve_layout "$1") || return 1

	# shellcheck disable=SC2034 # consumed by _yaml_get / _yaml_get_int
	local _tmux_layout_yaml
	_tmux_layout_yaml=$(envsubst <"$path")

	local session_name window_count
	session_name=$(_yaml_get '.session.name')
	if [ -z "$session_name" ]; then
		echo "tmux-layout switch: session.name is required in $path" >&2
		return 1
	fi
	window_count=$(_yaml_get_int '.windows | length')
	if ((window_count == 0)); then
		echo "tmux-layout switch: $path must declare at least one window" >&2
		return 1
	fi

	local mode target start_idx=0
	if [ -n "${TMUX:-}" ]; then
		mode=append
		target=$(tmux display-message -p '#S')
	else
		mode=session
		target=$session_name
		if tmux has-session -t "=$target" 2>/dev/null; then
			echo "tmux-layout: session '$target' already exists — attaching" >&2
			exec tmux attach-session -t "=$target"
		fi

		local w0_name p0_count p0_name p0_cmd p0_cwd p0_string
		w0_name=$(_yaml_get '.windows[0].name')
		p0_count=$(_yaml_get_int '.windows[0].panes | length')
		p0_name=""
		p0_cmd=""
		if ((p0_count > 0)); then
			p0_name=$(_yaml_get '.windows[0].panes[0].name')
			p0_cmd=$(_yaml_get  '.windows[0].panes[0].command')
		fi
		p0_cwd=$(_resolve_cwd 0 0)
		p0_string=$(_build_pane_cmd "$p0_cmd")

		local -a ns_args=(-d -s "$target" -n "${w0_name:-window}")
		[ -n "$p0_cwd" ] && ns_args+=(-c "$p0_cwd")
		if [ -n "$p0_string" ]; then
			tmux new-session "${ns_args[@]}" "$p0_string"
		else
			tmux new-session "${ns_args[@]}"
		fi

		local wid
		wid=$(tmux list-windows -t "$target" -F '#{window_id}' | head -n1)
		[ -n "$p0_name" ] && tmux select-pane -t "$wid" -T "$p0_name"
		_add_panes_after_first 0 "$wid"
		_apply_window_layout 0 "$wid"
		start_idx=1
	fi

	local i
	for ((i = start_idx; i < window_count; i++)); do
		_create_window_full "$i" "$target"
	done

	if [ "$mode" = "session" ]; then
		if [ -n "${TMUX:-}" ]; then
			tmux switch-client -t "$target"
		else
			exec tmux attach-session -t "$target"
		fi
	fi
}
