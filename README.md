# Tailscale Mullvad dmenu Selector

![Logo](https://github.com/user-attachments/assets/8f14ad69-2a44-40a4-8a8e-359f0ec5617f)


A simple dmenu-based selector to manage Tailscale exit nodes and custom actions. This tool allows you to quickly enable or disable Tailscale, set Tailscale exit nodes, and execute custom actions via a dmenu interface.

## Features

- Enable or disable Tailscale
- Set Tailscale exit nodes
- Execute custom actions
- Display country flags for exit nodes
- Customizable actions via a configuration file

## Installation

1. Ensure you have Rust installed. If not, you can install it from [rust-lang.org](https://www.rust-lang.org/).
2. Clone this repository:
   ```sh
   git clone https://github.com/yourusername/tailscale-dmenu-selector.git
   cd tailscale-dmenu-selector
   ```
3. Build the project:
   ```sh
   cargo build --release
   ```
4. Move the binary to a directory in your PATH:
   ```sh
   cp target/release/tailscale-dmenu-selector /usr/local/bin/
   ```

## Configuration

The configuration file is located at `~/.config/tailscale-dmenu/config.toml`. If it doesn't exist, a default configuration will be created automatically.

### Default Configuration

```toml
[[actions]]
display = "❌ - Disable mullvad"
cmd = "tailscale set --exit-node= --exit-node-allow-lan-access=false"

[[actions]]
display = "❌ - Disable tailscale"
cmd = "tailscale down"

[[actions]]
display = "✅ - Enable tailscale"
cmd = "tailscale up"

[[actions]]
display = "🌿 RaspberryPi"
cmd = "echo 'RaspberryPi action selected'"
```

You can add more actions by editing this file.

## Usage

Run the following command to open the dmenu selector:

```sh
tailscale-dmenu-selector
```

Select an action from the menu. The corresponding command will be executed.

## Dependencies

- [dmenu](https://tools.suckless.org/dmenu/)
- [Tailscale](https://tailscale.com/)
- [Rust](https://www.rust-lang.org/)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
