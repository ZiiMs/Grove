# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/ZiiMs/Grove/compare/v0.1.0...v0.1.1) - 2026-02-26

### Added

- *(ci)* split GoReleaser into separate Linux/macOS configs
- *(ci)* replace CircleCI with release-plz + GoReleaser
- add Linear PM setup wizard with guided token and team configuration
- *(notion)* support dynamic status options
- add open_editor keybind and restore keybind matching
- add customizable keybinds in settings panel

### Changed

- *(ci)* separate release-plz into its own workflow file

### Documentation

- add guide for implementing PM setup wizards
- add keybinds section to AGENTS.md

### Fixed

- *(ci)* rename .goreleaser.macos.yaml to .goreleaser-macos.yaml
- *(ci)* install zig before running cargo-zigbuild
- preserve task name, url and is_subtask when changing Linear status
- use just title for task name, remove identifier prefix
- use minimal ChildIssueData for children nodes parsing
- remove duplicate identifier from task name display
- truncate task name to 24 chars in agent list
- show full task name (identifier + title) in agent list, truncate status name
- show full Linear status name instead of truncated
- add status_name field to LinearTaskStatus for correct status display
- remove duplicate Linear handler and apply formatting
- restore keybind capture mode in settings

### Other

- Update .goreleaser-macos.yaml
- Update .goreleaser-macos.yaml
- Update .goreleaser-macos.yaml
- Update .goreleaser-macos.yaml
- Use split checksums to avoid upload conflicts between parallel builds
- Simplify macOS release: let GoReleaser build and publish in one step
- Update release.yml
- Remove zig version specification from release workflow
- Upgrade zig setup action to version 2
- Add Zig installation and clean option to release workflow
- Change single_build to true for grove binary
- Update .goreleaser-macos.yaml configuration
- Remove force_token from .goreleaser-linux.yaml
- Remove cargo-zigbuild installation from release workflow
- Set single_build to false in goreleaser config
- Add pull_request trigger to CI workflow
- Merge remote-tracking branch 'origin/main' into ziim/gre-48-more-robust-version-system
- Add .goreleaser.yaml
- Update gitignore
- Merge pull request #16 from ZiiMs/ziim/gre-7-redesign-checkout
- [GROVE] GRE-7 Redesign checkout
- [GROVE] GRE-43 Automation flow
- [GROVE] GRE-43 Automation flow
- [GROVE] GRE-41 Diff view
- [GROVE] GRE-41 Diff view
- [GROVE] GRE-41 Diff view
- [GROVE] GRE-41 Diff view
- [GROVE] Fix Linear team field missing in top-level issues query
- [GROVE] Fix Linear team field missing in nested children query
- Merge remote-tracking branch 'origin/main' into ziim/gre-36-fix-url-parsing
- [GROVE] GRE-36 Log team mismatch to TUI logs panel
- [GROVE] GRE-36 Log warning on Linear team mismatch
- [GROVE] GRE-36 Silent fail on Linear team mismatch
- [GROVE] GRE-36 Fix URL parsing
- [GROVE] GRE-36 Fix URL parsing
- Merge main into branch - resolve settings_modal.rs conflicts
- [GROVE] GRE-30 Create helper/utils section for reused code.
- [GROVE] GRE-30 Create helper/utils section for reused code.
- [GROVE] GRE-30 Create helper/utils section for reused code.
- Update gitignore
- Merge remote-tracking branch 'origin/main' into update-project-setup
- Merge main into fix-status
- [GROVE] Fix Status
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- [GROVE] Linear Git Integration
- Merge remote-tracking branch 'origin/main' into setup-for-git
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for GIT
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] Setup for PM
- [GROVE] linear subtasks
- [GROVE] linear subtasks
- [GROVE] linear subtasks
- [GROVE] linear subtasks
- [GROVE] linear subtasks
- [GROVE] Apply cargo fmt formatting
- [GROVE] Fix-linear
- [GROVE] Fix-linear
- [GROVE] Fix-linear
- [GROVE] Fix-linear
- [GROVE] Fix-linear
- Merge main into version-system
- [GROVE] Restore git-based versioning system
- [GROVE] Tidy up Status checks
- [GROVE] Tidy up Status checks
- [GROVE] Tidy up Status checks
- [GROVE] Tidy up Status checks
- [GROVE] Tidy up Status checks
- Fix build errors.
- [GROVE] Integrate Codex
- [GROVE] Integrate Codex
- [GROVE] Integrate Codex
- [GROVE] Integrate Codex
- [GROVE] Integrate Codex
- Merge remote-tracking branch 'origin/main' into integrate-codex
- [GROVE] Integrate Codex
- [GROVE] Integrate Codex
- [GROVE] Add remaining project management
- [GROVE] Add remaining project management
- [GROVE] Add remaining project management
- [GROVE] Add remaining project management
- [grove] paused 'Add remaining project management'
- Merge main into add-remaining-project-management
- [grove] paused 'Add remaining project management'
- [grove] paused 'Add remaining project management'
- [grove] paused 'Add remaining project management'
- [grove] paused 'Add remaining project management'
- [grove] paused 'Add remaining project management'
- [grove] paused 'Add remaining project management'
- Rename to grove
- Fix name so no compability issues/conflicks with flock on linux
- Merge main into keybinds branch
- Merge main into keybinds branch
- Merge remote-tracking branch 'origin/main' into keybinds
- Fix footer hint text after merge
- Merge remote-tracking branch 'origin/main' into promptsettings
- [flock] paused 'promptsettings'
- [flock] paused 'promptsettings'
- [flock] paused 'promptsettings'
- [flock] paused 'promptsettings'
- [flock] paused 'promptsettings'
- Merge remote-tracking branch 'origin/main' into promptsettings
- Fix tmux session ended.
- Agent.md update
- Workflow updates
- Add GitHub Actions workflow for Rust project
- Gitignore .flock
- Fix tiny error with project setup.
- install.sh
- Add Codeberg CI integration: Forgejo Actions and Woodpecker CI support
- Fix preview panel, and panic caused from UTF-8
- More settings, toggle panes and banners.
- Add github integration for MR, aswell as pipeline. While also maintaining Pipeline integration.
- Update detector to be per agent based.
- Changed checkout to not end, we attatch to a detatched head making our workflow a little more complicated but it means we can not delete worktrees.
- Before checkout change
- Initial commit
