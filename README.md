# nsticky

`nsticky` is a window management helper tool built on top of [niri](https://github.com/YaLTeR/niri). It focuses on managing **sticky windows** — windows fixed across all workspaces — to enhance your workflow efficiency.

## Features

✨ **Powerful Sticky Window Management:**
Easily fix windows across all workspaces to keep your most important apps visible at all times.

🔧 **Flexible Controls:**
Add or remove windows from the sticky list on demand via intuitive CLI commands.

📋 **Real-time Overview:**
Quickly list all currently sticky windows to stay organized.

⚡ **Instant Toggle:**
Toggle the sticky state of the currently active window with a single command or shortcut.

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

## Usage

### Daemon mode

Configure `niri` to auto-start the `nsticky` daemon:

```bash
spawn-at-startup "nsticky"
```

### Command line

Control `nsticky` from the terminal using CLI commands:

```bash
./target/release/nsticky add <window_id>          # Add a window to the sticky list
./target/release/nsticky remove <window_id>       # Remove a window from the sticky list
./target/release/nsticky list                      # List all sticky windows
./target/release/nsticky toggle-active             # Toggle sticky state of the active window
```

You can also set up a shortcut in `niri`:

```bash
Mod+Ctrl+Space { spawn "nsticky" "toggle-active"; }
```

---

## Design

`nsticky` communicates with its daemon via a Unix Domain Socket. The CLI client sends commands while the daemon manages sticky window states.

The daemon also listens to `niri`’s event stream to automatically handle window movement on workspace switches.

---

## Dependencies

🛠️ **Core Libraries:**

- **Tokio:** Asynchronous runtime for smooth, non-blocking IO.
- **Clap:** Robust command-line argument parser.
- **Anyhow:** Simplified error handling for better reliability.
- **Serde / serde_json:** Efficient JSON serialization and deserialization.

🔗 **Integration:**

- **niri:** The window manager integration foundation, enabling seamless event handling.

---

## Development

Contributions and feedback are welcome!
Please format code with `cargo fmt` and check with `cargo clippy`.

---

## License

This project is licensed under the BSD 3-Clause License.

---

> If you find `nsticky` useful, please give it a ⭐ and share! 🎉
