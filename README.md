# nsticky

`nsticky` is a window management helper tool built on top of [niri](https://github.com/YaLTeR/niri). It manages **sticky windows** — windows fixed across all workspaces — and **staged windows** — windows temporarily moved to a dedicated workspace. Niri has no native global sticky windows; `nsticky` lets you pin windows to every workspace and park them in a staging area without losing track of them.

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

## Configuration

Create `~/.config/nsticky/config.toml` to auto-sticky windows matching rules:

```toml
menu = "pantry -m"

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
    menu = "pantry -m";   # optional: selector for `stage restore`
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
nsticky stage restore                   # Pick staged window(s) to restore; pipes list to $NSTICKY_MENU (or config `menu`), else terminal prompt
```

`stage restore` is menu-agnostic and supports multi-select. It pipes the staged window list to an external selector, or falls back to a built-in terminal prompt.

<p align="center">
  <img src="assets/preview.png" alt="Preview" width="80%">
</p>

- Set the selector in `config.toml`: `menu = "pantry"` — or any dmenu-compatible command.
- The `NSTICKY_MENU` environment variable overrides the config value.
- The stdin list is `id\tapp_id — title`; the selected lines' ids are restored.
- Recommended selector: [pantry](https://github.com/lonerOrz/pantry.git).

You can set up shortcuts in `niri`:

```bash
Mod+Ctrl+Space { spawn "nsticky" "sticky" "toggle-active"; }
Mod+Shift+Space { spawn "nsticky" "stage" "toggle-active"; }
Mod+Shift+R { spawn "nsticky" "stage" "restore"; }   # pick staged window(s) to restore
```

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
