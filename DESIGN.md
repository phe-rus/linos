---
name: Linos
description: "Made to feel at home. Existing Linos 13 Server visual and interaction system."
---

# Design System: Linos

## Overview

**Linos design philosophy: Made to feel at home.**

Linos is a consumer-oriented operating-system family for ordinary people. Comfort includes understandable language, consistent interactions, accessibility, responsiveness, user control, and care across hardware and software. Apple's attention to those relationships is a quality reference, not an appearance to copy. Server is the first edition; Desktop, mobile, television and other supported devices will require their own expressions of this philosophy.

**Creative North Star for the current Server interface: "The Familiar Workbench"**

Linos 13 Server pairs an approachable, personal workspace with the calm precision of a quiet control room. The existing interface uses sparse terminal composition, restrained color, direct keyboard actions and guided setup. Its visual voice is calm and welcoming; its controls are precise and restrained. This metaphor describes Server today, not the identity or eventual appearance of every Linos edition.

**Key Characteristics:**

- A persistent bottom command field and a spacious central workspace.
- Cyan identity and command cues, quiet secondary text and explicit selection markers.
- Guided first boot and local sign-in, with standard system password handling.
- Read-only inspection views with a native Shell escape for advanced work.
- Terminal-dependent typography and colors rather than a fixed graphical theme.

### Scope and evidence

**23 November interaction continuation:** Home now uses terminal-default foreground text, supports a monochrome preference, accepts repeated editing keys and Unicode cursor movement, and provides measured resource bars, live task controls and working Settings changes. System actions have centered confirmation panels and normal authorization. The 14 November observations below remain a historical snapshot; current behavior and terminal/guest evidence are recorded in [the Server interaction review](docs/reviews/2026-11-23-server-interaction.md).

Captured on 14 November 2026 from `main` at `8cce91d`, with pre-existing local changes restored after the pull. This records source behavior, not a release certification. No UI redesign was performed as part of documentation; source changes during the pull restore local work and reconcile upstream updates.

Product authorities: [PRODUCT.md](PRODUCT.md), [vision](docs/vision.md), [architecture](docs/architecture/overview.md) and [master development plan](docs/plans/linos13-master-plan.md). [Engineering status](docs/architecture/current-status.md) reports earlier VM demonstrations; those screenshots are not committed here and were not independently inspected in this pass. Historical evidence is not proof of the current combined source.

Implementation authorities: [Home and interaction](system/src/console/tui.rs), [CLI entry](system/src/console/main.rs), [terminal geometry](system/src/console/geometry.rs), [Assets](system/src/console/assets.rs), [setup](system/src/setup/ui.rs), [sign-in](system/src/login.rs), [PTY](system/src/console/pty.rs), [Hardware](system/src/console/hardware.rs), [Linus store](system/src/console/store.rs) and [Recovery](system/src/recovery/main.rs). The lockfile resolves Ratatui 0.30.2, Crossterm 0.29.0 and Cliclack 0.5.7; setup and login do not supply a custom Cliclack theme.

| Evidence level | What it establishes |
| --- | --- |
| Source observed | Layout arithmetic, styling, event handlers, prompt configuration and read-only operations described below. |
| Previously reported VM evidence | Setup, sign-in, Home, Assets, Resources, Settings and resizing, as recorded in engineering status. |
| Source only / unverified here | Hardware, Linus store, newer PTY password handling and the merged working tree. |
| 14 November validation blocker | The snapshot's `cargo test --workspace --locked` failed at `system/src/console/pty.rs:147` with an unterminated character literal. See the 23 November continuation above for current successful validation. |

The frontmatter intentionally has no CSS color, font-size, radius or spacing scales. This native implementation uses ANSI color roles and terminal cells; inventing hex colors or pixel tokens would create a false source of truth. The sidecar records native measurements and clearly labeled browser illustrations, not a new runtime theme.

## Colors

### Primary

**Workbench Cyan** is the descriptive name for Home's `Color::Cyan` accent: identity, slash-command examples, selected command text, command prompt and the command-field border when input is nonempty. The exact displayed color comes from the terminal palette.

### Neutral

| Descriptive name | Source role | Existing use |
| --- | --- | --- |
| Clear text | `Color::White` | Welcome/title text, view output, selected suggestion description and entered command text. |
| Quiet text | `Color::Gray` (`DIM`) | Identity details, secondary copy, location, placeholders, inactive suggestions and footer. `DIM` names a color constant, not a dim modifier. |
| Frame gray | `Color::DarkGray` | Overview border and empty command-field border. |
| Terminal canvas | Unspecified background / default style | Background and inherited foreground, including raw overview values and block titles. No app-owned light/dark palette switch exists. |

### Guided prompt feedback

The resolved Cliclack default theme adds semantic colors: cyan active rails; green submitted-step symbols and selected radio markers; yellow validation/error-state rails and warning symbols; red cancellation and error-log symbols; blue informational log symbols. Completed rails use bright black; placeholders, hints and submitted input use dim styling. These are dependency defaults, not an extracted Linos-wide palette. Recovery's numbered menu uses plain output.

**The Terminal Palette Rule.** Preserve native color roles when documenting Server. A terminal's theme determines their appearance; fixed contrast ratios and graphical palette values remain unverified.

## Typography

**Display, body and label rendering:** the user's terminal font, size, line height, glyph coverage and cell metrics. The app does not select a font family or ship a typographic size scale. All text occupies the same terminal grid; hierarchy comes from bold, color, spacing and wording.

| Role | Existing treatment |
| --- | --- |
| Identity | Bold cyan `◆  Linos`, followed by gray generation, edition and `user@host`. |
| Welcome / view title | Bold white; welcome has a gray supporting line. |
| Body and output | White or inherited foreground; plain text, aligned labels and units. |
| Secondary / helper | Gray in Home; dim dependency styling in guided prompts. |
| Section / field title | Block titles and inline headings; casing is mixed across views. |
| Data | Monospaced terminal alignment; Assets truncates names to 39 Unicode scalar values and formats a 42-character name column. |

Canonical product names are **Linos**, **Linos 13**, **Assets** and **Linus store**. Some current strings still use uppercase headings, including `ASSETS`, `QUICK ACCESS`, `LARGEST PACKAGES` and `PREVIEW ONLY`. These are recorded inconsistencies, not the preferred product voice. Setup and account validation messages remain primarily English despite offering a language preference.

## Layout

### Home geometry (terminal cells)

The full-screen Ratatui workspace uses the alternate screen. Let terminal width be `W`, height be `H`, and coordinates be relative to the frame origin.

| Element | Existing placement / size |
| --- | --- |
| Workspace | Width `W−2` at widths through 100 columns; `W−8` above 100; horizontally centered, without a maximum width. |
| Inner content | Two columns inset from the workspace on each side. |
| Identity | Row 1; one row high. |
| Welcome / view title | Row 4; welcome uses two rows. |
| Overview | Row 8, six rows high, including border; four label/value rows inside. |
| Command field | Top `H−5`, three rows high including borders; input on its middle row. |
| Location | Row immediately above the command field. |
| Footer | Final terminal row. |
| View output | Starts at row 7; wraps without trimming leading whitespace and scrolls vertically. |
| Suggestions | Up to five one-row entries above the location line, limited further by available height. |

The overview appears only when `composer_y > hero_y + 11` (at least 21 rows with the present offsets). The Explore section requires `composer_y > status_y + 9` (at least 23 rows). These are height-dependent omissions, not collapsed interactive cards.

Below 36 columns **or** 14 rows, the renderer shows a resize message instead of the workspace. It is not a usable compact layout. The message currently advertises `Ctrl+C: shell`, but the key handler uses Ctrl+T for Shell; Ctrl+C sends a PTY interrupt when applicable.

### Resizing and density

Crossterm resize events update PTY dimensions using `rows−12` and `columns−4`. Serial terminals can be queried with CSI 18t; accepted reports range from 36–1000 columns and 14–500 rows. Serial geometry is rechecked every four seconds while input is empty. Ctrl+R requests recalibration; unsupported reporting produces a notice with manual `stty` guidance. These are source mechanisms, not proof of every terminal's compatibility.

Assets and Linus store retain fixed-width text columns inside a wrapping output region. Long paths, usernames, output and footer hints can clip or wrap awkwardly. No horizontal scrolling, responsive table-column model or pointer/touch layout is implemented. Cliclack wraps prompt content using terminal width; Linos does not configure a shared grid or explicit maximum option rows for its setup selections.

## Elevation & Depth

Server is flat. There are no app-defined shadows, gradients, blur, translucent layers or elevation tokens. Hierarchy uses whitespace, foreground emphasis, one-cell outlines and the vertical sequence of guided prompts. Suggestions occupy the workspace rather than floating in a shadowed overlay. Power confirmation replaces footer text; it is not a modal dialog.

No authored animation, easing curve or interaction-physics system exists in Home. Event polling uses 100 ms intervals; Resources refreshes every two seconds while open, and drawing is gated by a dirty flag. These are scheduling choices, not measured input-latency or frame-rate guarantees. No app-specific reduced-motion preference exists.

## Shapes

Home's overview and composer use Ratatui's default square, single-line all-sided borders. The composer is three terminal rows high. There are no rounded cards, pill controls or app-owned icon assets in these views.

Identity/footer diamonds (`◆`), suggestion marker (`›`), Assets selection (`❯`), password bullets (`•`), arrows and resource blocks (`█`/`░`) are text glyphs. They rely on terminal font support. Cliclack uses connected line/step symbols with dependency-provided fallback glyphs. Its notes can use a different border vocabulary; this is not a unified Linos shape library.

Brand artwork exists under `assets/brand/`; the current terminal renderer does not display those raster assets. Their presence does not establish graphical-edition tokens.

## Components

### Command composer

A persistent outlined text field anchors Home. Empty input has a gray border and helper text; nonempty input makes the border cyan and the text white. This is an **input-content state**, not a complete focus model. The terminal cursor follows the input and is clamped at the right border. Entry appends characters, Backspace removes the last character, and input is limited to 100 Unicode scalar values. Left/right editing, selection and a horizontal text viewport are absent.

Typing `/` filters commands by prefix. Enter executes the selected suggestion or recognized action; ordinary text is submitted to the persistent PTY. Escape clears input and resets the notice. Ctrl+L clears the transcript. `clear` and `/clear` have related transcript-clearing paths. Secrets are intended to display bullets and suppress suggestions when password mode is active; newer source combines command-pending state, prompt-text detection and PTY echo state. This does not establish correct secret handling: the engineering record reports false password mode, and current source does not compile.

### Command suggestions and navigation

Suggestions use a textual marker plus cyan command/white description for the selected row; other rows are gray. Up moves toward the first item; Down and Tab move toward the last, without wrapping. Only the first five matches render: there is no moving window when the selected index advances beyond those visible rows. Navigation is slash-command driven, with Home examples and `/help`; there is no persistent sidebar, tab bar or breadcrumb navigation control.

PageUp/PageDown scroll output in seven-row increments. `/home` returns to the workspace. `/shell` and Ctrl+T restore the terminal and start a real child shell; exiting returns to Home. The embedded PTY and native Shell have separate working-directory state. `/logout` exits via a dedicated session code. Full-screen programs belong in native Shell; the embedded output is a plain-text preview, not an ANSI terminal emulator.

### Identity, overview and feedback

The identity row combines the product, edition and account/host. The overview displays Device, Uptime, Memory and Setup. The footer combines a textual notice with command, Shell and resize hints. Failures and cancellations use text such as permission denial or folder-open errors; they do not have a unified error panel or severity color system. Several system queries are synchronous, with no common loading, cancellation or retry component.

### Assets

Read-only directory browsing opens in the user's home directory and retains its directory when leaving the view. With an empty composer, Up/Down selects entries, Enter opens directories or reports file selection, and Backspace opens the parent. `/assets/path` jumps to an absolute directory. Directories sort first, then case-insensitive names. Rows show Name, Type and Size, with a textual selection marker. The quick-access line is explanatory text, not a set of focusable controls.

An empty directory has an explicit message; open failures go to the footer. Rendering is capped at 500 entries with a notice when more exist; selection can still advance beyond that displayed subset. Files are not executed, previewed, moved or deleted. Directory-entry errors may be silently skipped and initial refresh errors are ignored, so empty and failed states are not consistently distinct.

### Resources, Settings, Hardware and Linus store

Resources uses 28-cell filled/empty bars for memory, swap and root storage, with percentages and numeric units. Zero capacity says `Not configured`; load averages are labeled separately from CPU utilization. Settings is read-only information. Updates explicitly says automatic OS updates and rollback are not implemented.

Hardware is a source-only Linux-reported inventory, not compatibility certification. Linus store is a source-only dpkg/APT browser with installed inventory, repository search, details and simulated install/remove previews. Labels must preserve **Linus store**. A preview does not install software; authenticated managed transactions, progress and recovery are absent. Inventory parsing can turn query failures into an apparently empty list, and output is capped; completeness is not guaranteed.

### Setup and sign-in

Cliclack supplies sequential input, select, review-note, confirmation and completion patterns. Setup asks for username, delegates password entry to the system, then collects language, keyboard, timezone, hostname and optional DHCP. Text validation explains accepted username/hostname characters. Configuration review defaults to **Yes**; this is distinct from power confirmation's default cancellation. Setup is not necessarily side-effect-free before the final preference review: account/password work occurs earlier in enrollment.

Local sign-in uses a remembered public username when available, otherwise asks for one. It delegates authentication to `/bin/login` and PAM; it does not collect the password in a custom Linos field. The login program checks interactive streams but does not share setup's `TERM=dumb`/`NO_COLOR` gate.

### Power confirmation and plain interfaces

Restart/shutdown sets an inline confirmation notice. Y confirms; Enter, Escape or N cancels. Authorization is delegated to systemd; denial leaves the session running. These are not graphical buttons.

Setup explicitly falls back to plain prompts when streams are noninteractive, `TERM=dumb`, or `NO_COLOR` is present. Home checks interactive streams and `TERM`, but does **not** check `NO_COLOR`. Recovery retains a separate numbered plain-text menu. Do not infer a single fallback or focus contract across all three implementations.

### Focus and accessibility: existing support and limits

Keyboard operation, visible textual selection, labeled metrics, written notices and default-cancel power actions provide useful non-color cues. Cliclack supplies focused prompt state, a reverse-video text cursor and validation feedback. Home keeps the composer cursor visible while arrow keys may control suggestions or Assets.

There is no demonstrated screen-reader contract for the alternate-screen workspace, assistive announcement mechanism, complete focus traversal, contrast certification, configurable text scale, high-contrast mode or graphical accessibility tree. Mouse/touch/remote navigation is absent. Unicode scalar counts used for field length, cursor position and filename truncation are not grapheme/display-cell measurements; combining marks and wide characters remain risks. Product accessibility requirements must not be described as current compliance.

## Do's and Don'ts

- Do keep Linos's “Made to feel at home” philosophy distinct from the Server-specific Familiar Workbench expression.
- Do preserve canonical names: Linos, Linos 13, Assets and Linus store.
- Do preserve terminal ownership and restore it when handing control to native Shell.
- Do describe read-only data, simulations and unavailable operations explicitly.
- Do distinguish source implementation, historical demonstrations and current verification.
- Don't promote ANSI colors, terminal typography or slash navigation into universal standards for future editions.
- Don't present dependency-default styling as an authored Linos-wide component library.
- Don't claim correct password handling, accessibility compliance or measured responsiveness without matching evidence.
- Don't silently elevate privileges or treat enrollment as blanket authorization.
- Don't redesign existing screens as a side effect of documenting them.

## Improvements needed to meet the Linos philosophy

**This section is future work, not implemented standards.** It is separate from the observed system above. Priorities follow the master plan; proposed acceptance checks below need implementation and real terminal/device validation.

| Priority | Observed gap | Needed improvement and evidence |
| --- | --- | --- |
| P0 | Pulled `pty.rs:147` contains an incomplete character literal. | Restore a compiling baseline; run format, Clippy and workspace tests before claiming current behavior. |
| P0 | False password mode was reported; newer source relies on prompt heuristics. | Establish reliable PTY state/secret handling. Validate idle prompts, changed prompts, repeated commands, `sudo`, cancellation and return to Home without recording secrets. |
| P1 | Current sources cannot be rendered; historical screenshots are not committed here. | Rebuild an immutable guest and capture versioned Home, setup, sign-in, Assets, Resources, Hardware and Linus store evidence. |
| P1 | Resize fallback advertises Ctrl+C although Shell uses Ctrl+T; fixed columns and footer can clip. | Correct guidance and exercise tiny, minimum, standard and wide layouts, long paths, non-Latin text and serial/SSH resize. Existing TestBackend cases are 30×10, 48×18, 80×24, 120×32 and 160×42; they are not visual or accessibility certification. |
| P1 | One resize test still expects uppercase `LINOS` after upstream changed the identity to `Linos`. | Reconcile stale assertions with canonical casing and add meaningful layout/state regression evidence after compilation is restored. |
| P1 | Suggestion selection can leave the visible five; Assets selection can exceed the rendered 500. | Keep selected items visible and operable, with explicit list limits and tested keyboard navigation. |
| P1 | Cursor/name widths count scalar values; composer lacks full editing. | Use grapheme/display-cell-aware measurements and define predictable editing, scrolling and focus behavior. |
| P1 | `NO_COLOR`, plain output and sign-in checks differ; assistive support is unverified. | Agree and test terminal capability/fallback contracts, contrast under supported palettes, non-color feedback and screen-reader behavior. |
| P1 | Uppercase labels, English-only prompts and toolkit styles are inconsistent. | Reconcile language, capitalization and shared state vocabulary without changing canonical product names; test localization and expansion. |
| P1 | Read errors can look empty; synchronous commands lack consistent loading/retry. | Distinguish loading, empty, partial, unavailable and failed data; retain context and give recoverable next actions. |
| P1 | Settings edits, package transactions and recovery remain incomplete. | Design explicit permissions, previews, progress, cancellation and recovery before exposing mutations. |
| P2 | No measured interaction physics or cross-device component contracts exist. | Prototype edition-appropriate typography, focus/input models, motion, reduced motion and hardware-aware performance budgets. Measure whole-system responsiveness; do not invent timings or select a graphical framework in this record. |

Server should establish reusable contracts for clarity, feedback, ownership, permission and recovery. Desktop, mobile, television and other supported editions should express those contracts through suitable native inputs and layouts. Their fonts, palette, geometry, animation and accessibility acceptance criteria remain design and engineering work, not inherited terminal defaults.
