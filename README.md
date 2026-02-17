# Detail CLI

> ⚠️ **NOTE**: This CLI is currently in alpha. Features and commands may change.

Command-line interface for [Detail](https://detail.dev).

## Installation

### macOS/Linux/Windows

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://cli.detail.dev | sh
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
## Usage

See the full [command-line reference](docs/HELP.md) for detailed usage of every command and option.

Use `--help` with any command for quick reference:

```bash
detail --help
detail bugs --help
```
