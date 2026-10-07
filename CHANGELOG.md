# Changelog

## [0.5.0](https://github.com/tmux-contrib/tmux-layout/compare/v0.4.0...v0.5.0) (2026-10-07)


### Features

* show list as a table and offer to re-open invalid layouts in edit ([#15](https://github.com/tmux-contrib/tmux-layout/issues/15)) ([fb4fdc3](https://github.com/tmux-contrib/tmux-layout/commit/fb4fdc3e400f44e76cf735690ac6d040d4f92eb2))

## [0.4.0](https://github.com/tmux-contrib/tmux-layout/compare/v0.3.0...v0.4.0) (2026-10-06)


### Features

* add save command to capture a tmux session as a layout ([#13](https://github.com/tmux-contrib/tmux-layout/issues/13)) ([612d260](https://github.com/tmux-contrib/tmux-layout/commit/612d26010ccb5a306121494cd030c314735d6534)), closes [#10](https://github.com/tmux-contrib/tmux-layout/issues/10)

## [0.3.0](https://github.com/tmux-contrib/tmux-layout/compare/v0.2.0...v0.3.0) (2026-10-06)


### Features

* add new and edit commands ([#11](https://github.com/tmux-contrib/tmux-layout/issues/11)) ([4614983](https://github.com/tmux-contrib/tmux-layout/commit/46149832a8e7a5b00c789504a8d33ecfc640a625)), closes [#8](https://github.com/tmux-contrib/tmux-layout/issues/8) [#9](https://github.com/tmux-contrib/tmux-layout/issues/9)

## [0.2.0](https://github.com/tmux-contrib/tmux-layout/compare/v0.1.0...v0.2.0) (2026-10-06)


### ⚠ BREAKING CHANGES

* the zsh plugin is removed, since it only added the repository to PATH. Install the binary with Nix, from a release, or with cargo install --git instead.

### Features

* rewrite the CLI in Rust ([#6](https://github.com/tmux-contrib/tmux-layout/issues/6)) ([8d53d7c](https://github.com/tmux-contrib/tmux-layout/commit/8d53d7c7b9e11cd29df335e9254969818b86e67b))

## [0.1.0](https://github.com/tmux-contrib/tmux-layout/compare/v0.0.1...v0.1.0) (2026-05-13)


### Features

* add tmux-layout CLI tool for YAML-defined layouts ([657987e](https://github.com/tmux-contrib/tmux-layout/commit/657987e16966adb6355600cd1b76b49cf398cc42))
* add working directory support for sessions, windows, and panes ([0a50c4f](https://github.com/tmux-contrib/tmux-layout/commit/0a50c4f13038ab1a86569ece44a6232c118af43d))


### Bug Fixes

* preserve YAML window order when applying layouts ([c26c4ee](https://github.com/tmux-contrib/tmux-layout/commit/c26c4eee72e66e840d0a6e77f96b259d81630285))
* run nix-develop pane commands through user's shell ([1c1e5ff](https://github.com/tmux-contrib/tmux-layout/commit/1c1e5ffbfb4338fbf84ae146325deacdd10fd563))
