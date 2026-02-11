# Detail CLI

> ⚠️ **NOTE**: This CLI is currently in alpha. Features and commands may change.

Command-line interface for [Detail](https://detail.dev).

## Installation

### macOS/Linux/Windows

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/usedetail/cli/releases/latest/download/detail-installer.sh | sh
```

### From Source

```bash
cargo install --path .
```

## Authentication

The Detail CLI requires an API token to operate. You can generate an API token from the [Detail web UI](https://detail.dev) in your account settings.

Once you have your token, authenticate with:

```bash
detail auth login
```

You can check your authentication status at any time:

```bash
detail auth status
```

## Usage

The CLI provides commands for managing bugs and repositories. Use `--help` with any command to see detailed usage information:

```bash
detail --help
detail bugs --help
detail bugs list --help
```

## Configuration

Configuration is stored in:
- macOS: `~/Library/Application Support/com.detail.cli/config.toml`
- Linux: `~/.config/detail/cli/config.toml`
- Windows: `%APPDATA%\detail\cli\config.toml`

API tokens are stored securely in your system's native credential store:
- macOS: Keychain
- Linux: Secret Service
- Windows: Credential Manager

### Updates

The CLI automatically checks for updates once per day, and installs if found. 
To disable automatic updates, add this to your config file:

```toml
check_for_updates = false
```

You can run `detail-update` to manually update the CLI to the latest version.

### Environment Variables

- `DETAIL_API_URL` - Override the API endpoint (for testing)
