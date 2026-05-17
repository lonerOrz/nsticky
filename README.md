# nsticky

`nsticky` is a window management helper tool built on top of [niri](https://github.com/YaLTeR/niri). It focuses on managing **sticky windows** — windows fixed across all workspaces — and **staged windows** — windows temporarily moved to a dedicated workspace — to enhance your workflow efficiency.

## Why?

Niri doesn't natively support global sticky windows.
This tool allows you to designate certain windows to persist on every workspace, mimicking sticky behavior from other window managers. Additionally, it provides a staging area for temporarily hiding windows without losing track of them.

## Features

✨ **Powerful Sticky Window Management:**
Easily add/remove windows across all workspaces to keep your most important apps visible at all times.

📦 **Window Staging:**
Move sticky windows to a dedicated "stage" workspace to temporarily hide them, and restore them when needed.

📋 **Organized CLI Commands:**
Commands are logically grouped into `sticky` and `stage` categories for intuitive usage.

⚡ **Real-time Toggle:**
Quickly toggle the sticky/stage state of the currently active window with a single command.

🔍 **State Consistency:**
Atomic operations ensure internal state stays synchronized with actual window positions.

🔧 **Robust Error Handling:**
Failures during window operations are handled gracefully with state rollbacks.

---

## Installation

Make sure you have Rust installed along with the required `niri` tool.

### 1. Build from source

```bash
git clone https://github.com/lonerOrz/nsticky.git
cd nsticky
cargo build --release
```

### 2. Install via Nix (for Nix or NixOS users)

```bash
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nsticky.url = "github:lonerOrz/nsticky";
  };

  outputs =
    inputs@{
      self,
      flake-utils,
      nixpkgs,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [ inputs.nsticky.packages.${system}.nsticky ];
        };
      }
    );
}
```

### 3. Use precompiled binaries directly

---

## Configuration

Create `~/.config/nsticky/config.toml` to auto-sticky windows matching rules:

```toml
[sticky.firefox]
app-id = "firefox"

[sticky.kitty]
app-id = "kitty"
title = ".*server.*"

[sticky.gmail]
title = ".*Gmail.*"
```

**Matching rules:**
- `app_id` and `title` are AND logic (both must match)
- Use regex patterns
- If only one field is specified, it matches any value

You can also configure nsticky from the home-manager module:
```nix
{ inputs, ... }:

{
  imports = [
    inputs.nsticky.homeModules.default
  ];

  programs.nsticky = {
    enable = true;
    settings = {
      sticky = {
        firefox.app-id = "firefox";

        kitty = {
          app-id = "kitty";
          title = ".*server.*";
        };

        gmail.title = ".*Gmail.*";
      };
    };
  };
}
```

---

## Usage

### Daemon mode

Configure `niri` to auto-start the `nsticky` daemon:

```bash
spawn-at-startup "nsticky"
```

### Command line

Control `nsticky` from the terminal using grouped CLI commands:

#### Sticky Window Management:

```bash
nsticky sticky add <window_id>          # Add a window to the sticky list
nsticky sticky remove <window_id>       # Remove a window from the sticky list
nsticky sticky list                     # List all sticky windows
nsticky sticky toggle-active            # Toggle sticky state of the active window
nsticky sticky toggle-appid <appid>     # Toggle sticky state of window by application ID
nsticky sticky toggle-title <title>     # Toggle sticky state of window by title
```

> **Note:** The stage commands require a "stage" workspace to be defined in your Niri config:
>
> ```nix
> workspace "stage" {
>     open-on-output = "eDP-1"  # optional: specify which output
> }
> ```

#### Stage Window Management:

```bash
nsticky stage list                      # List all currently staged windows
nsticky stage add <window_id>           # Move a sticky window to the "stage" workspace
nsticky stage remove <window_id>        # Move a staged window back to the current workspace
nsticky stage toggle-active             # Toggle stage state of the active window (if in sticky, moves to stage; if in stage, moves back)
nsticky stage toggle-appid <appid>        # Move window with app ID to stage (if sticky) or back to current workspace (if staged)
nsticky stage toggle-title <title>        # Move window with title to stage (if sticky) or back to current workspace (if staged)
nsticky stage add-all                   # Move all sticky windows to the "stage" workspace
nsticky stage remove-all                # Move all staged windows back to the current workspace
```

You can set up shortcuts in `niri`:

```bash
Mod+Ctrl+Space { spawn "nsticky" "sticky" "toggle-active"; }
Mod+Shift+Space { spawn "nsticky" "stage" "toggle-active"; }
```

---

## Design

`nsticky` follows a modular architecture with clear separation of concerns:

### Core Modules:

- **main.rs**: Entry point, starts either CLI or daemon mode
- **cli.rs**: Parses and sends commands to the daemon
- **daemon.rs**: Handles incoming CLI commands and Niri events
- **business.rs**: Implements core business logic with state management
- **protocol.rs**: Defines command parsing and response formatting
- **system_integration.rs**: Handles communication with the Niri window manager

### State Management:

- **Sticky Windows**: Windows that appear on every workspace
- **Staged Windows**: Windows temporarily moved to a dedicated "stage" workspace
- Atomic operations ensure state consistency during window management operations

The daemon communicates with its CLI via a Unix Domain Socket at `/tmp/niri_sticky_cli.sock`.
The daemon also listens to `niri`'s event stream to automatically handle window movement on workspace switches.

---

## Dependencies

🛠️ **Core Libraries:**

- **Tokio:** Asynchronous runtime for smooth, non-blocking IO.
- **Clap:** Robust command-line argument parser for structured commands.
- **Anyhow:** Simplified error handling for better reliability.
- **Serde / serde_json:** Efficient JSON serialization and deserialization.

🔗 **Integration:**

- **niri:** The window manager integration foundation, enabling seamless event handling.

---

## Notes

- `nsticky` relies on the `niri` window manager.
- The daemon requires the `NIRI_SOCKET` environment variable to connect to Niri.
- The staging feature moves windows to a workspace named "stage". Ensure this workspace exists in your Niri configuration.
- Window IDs can be obtained using `niri msg --json windows`

---

## Development

Contributions and feedback are welcome!
Please format code with `cargo fmt` and check with `cargo clippy`.

---

## License

This project is licensed under the BSD 3-Clause License.

---

> If you find `nsticky` useful, please give it a ⭐ and share! 🎉
