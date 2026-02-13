# Detail CLI

> ⚠️ **NOTE**: This CLI is currently in alpha. Features and commands may change.

Command-line interface for [Detail](https://detail.dev).

## Installation

### macOS/Linux/Windows

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/usedetail/cli/releases/latest/download/detail-cli-installer.sh | sh
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

The CLI provides commands for managing bugs and repositories. Use `--help` with any command to see detailed usage information.

### Bug Management

```bash
# List bugs for a repository
detail bugs list owner/repo --status pending

# Show details for a specific bug
detail bugs show bug_abc123

# Close a bug (mark as resolved)
detail bugs close bug_abc123 --state resolved

# Dismiss a bug with a note
detail bugs close bug_abc123 --state dismissed --dismissal-reason not-a-bug --notes "Example note"
```

### Repository Management

```bash
# List all repositories you have access to
detail repos list
```

### Output Formats

All list commands support multiple output formats:

```bash
# Table format (default)
detail bugs list owner/repo --format table

# JSON format
detail bugs list owner/repo --format json

# CSV format
detail bugs list owner/repo --format csv
```

## Configuration

Configuration and API tokens are stored in `config.toml` at:
- macOS/Linux: `~/.config/detail-cli/config.toml`
- Windows: `%LOCALAPPDATA%\detail-cli\config.toml`

You can override the config directory by setting the `XDG_CONFIG_HOME` environment variable.

### Updates

The CLI automatically checks for updates once per day, and installs if found. 
To disable automatic updates, add this to your config file:

```toml
check_for_updates = false
```

You can run `detail-cli-update` to manually update the CLI to the latest version.

### Environment Variables

- `DETAIL_API_URL` - Override the API endpoint (for testing)
