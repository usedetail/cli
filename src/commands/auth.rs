use std::borrow::Cow;
use std::collections::HashMap;
use std::str::from_utf8;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use clap::Subcommand;
use console::{style, Term};
use percent_encoding::percent_decode_str;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Buffer size used to read the OAuth PKCE HTTP callback request.
/// 4 KiB is sufficient for a small request line plus headers.
const CALLBACK_BUFFER_SIZE: usize = 4096;
use tokio::time::timeout;

use crate::api::client::{pkce_token_exchange, ApiClient};
use crate::config::storage;

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Login with your Detail account
    Login {
        /// API token (`dtl_live_...`) — skips the browser flow
        #[arg(long)]
        token: Option<String>,
    },

    /// Logout and remove stored credentials
    Logout,

    /// Show current authentication status
    Status,
}

pub async fn handle(command: &AuthCommands, cli: &crate::Cli) -> Result<()> {
    match command {
        AuthCommands::Login { token } => {
            let config = storage::load_config()
                .inspect_err(|e| {
                    let _ = Term::stderr().write_line(&format!(
                        "Warning: Config file has errors, using default settings: {e}"
                    ));
                })
                .unwrap_or_default();
            let api_url = config
                .api_url
                .as_deref()
                .unwrap_or("https://api.detail.dev");
            let app_url = config
                .app_url
                .as_deref()
                .unwrap_or("https://app.detail.dev");

            let token = if let Some(t) = token {
                if !t.starts_with("dtl_") {
                    bail!("Invalid token format. Token should start with 'dtl_'");
                }
                t.clone()
            } else {
                pkce_login(api_url, app_url).await?
            };

            let client = ApiClient::new(config.api_url, Some(token.clone()))?;

            let user_info = client
                .get_current_user()
                .await
                .context("Failed to authenticate. Please check your token.")?;

            storage::store_token(&token)?;

            let term = Term::stdout();
            term.write_line(&format!(
                "{}",
                style("✓ Successfully authenticated!").green()
            ))?;
            term.write_line(&format!("Logged in as: {}", user_info.email))?;
            term.write_line("\nExample commands:")?;
            term.write_line("  detail bugs list <owner>/<repo>")?;
            term.write_line("  detail bugs show <bug_id>")?;

            Ok(())
        }

        AuthCommands::Logout => {
            storage::clear_credentials()?;
            Term::stdout()
                .write_line(&format!("{}", style("✓ Logged out successfully").green()))?;
            Ok(())
        }

        AuthCommands::Status => {
            if let Ok(client) = cli.create_client() {
                let term = Term::stdout();
                match client.get_current_user().await {
                    Ok(user) => {
                        term.write_line(&format!("{}", style("✓ Authenticated").green()))?;
                        term.write_line(&format!("Email: {}", user.email))?;
                    }
                    Err(e) => {
                        term.write_line(&format!("{}", style("✗ Authentication invalid").red()))?;
                        term.write_line(&format!("Error: {e}"))?;
                        term.write_line("\nRun `detail auth login` to re-authenticate")?;
                    }
                }
            } else {
                let term = Term::stdout();
                term.write_line(&format!("{}", style("✗ Not authenticated").red()))?;
                term.write_line("\nRun `detail auth login` to authenticate")?;
            }
            Ok(())
        }
    }
}

async fn pkce_login(api_url: &str, app_url: &str) -> Result<String> {
    // Generate code_verifier: 32 random bytes → 43-char base64url string (RFC 7636 compliant)
    let verifier_bytes: [u8; 32] = rand::random();
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    // code_challenge = BASE64URL(SHA256(code_verifier))
    let challenge_bytes = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(challenge_bytes);

    // State token for CSRF protection
    let state_bytes: [u8; 16] = rand::random();
    let state = URL_SAFE_NO_PAD.encode(state_bytes);

    // Bind a local listener; port 0 lets the OS pick a free port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to bind local callback listener")?;
    let port = listener.local_addr()?.port();

    // redirect_uri must be percent-encoded as a query parameter
    let encoded_redirect = format!("http%3A%2F%2F127.0.0.1%3A{port}%2Fcallback");
    let browser_url = format!(
        "{app_url}/cli-auth?redirect_uri={encoded_redirect}&state={state}&code_challenge={code_challenge}"
    );

    let term = Term::stdout();
    if open::that(&browser_url).is_ok() {
        term.write_line("Browser opened for authentication.")?;
    } else {
        term.write_line("Could not open browser automatically.")?;
    }
    term.write_line(&format!("  {browser_url}"))?;
    term.write_line(&format!(
        "\nTip: You can also generate an API key at {app_url}/cli and authenticate directly:"
    ))?;
    term.write_line("  detail auth login --token <your-api-key>")?;
    term.write_line("\nWaiting for authentication in the browser...")?;

    let (code, mut stream) = await_pkce_callback(listener, &state, app_url).await?;

    match pkce_token_exchange(api_url, &code, &code_verifier).await {
        Ok(token) => {
            redirect_browser(&mut stream, &format!("{app_url}/cli-auth/success")).await;
            Ok(token)
        }
        Err(e) => {
            redirect_browser(&mut stream, &format!("{app_url}/cli-auth/error")).await;
            Err(e)
        }
    }
}

async fn await_pkce_callback(
    listener: TcpListener,
    expected_state: &str,
    app_url: &str,
) -> Result<(String, TcpStream)> {
    let (mut stream, _) = timeout(Duration::from_mins(10), listener.accept())
        .await
        .context("Authentication timed out after 10 minutes")??;

    let mut buf = vec![0_u8; CALLBACK_BUFFER_SIZE];
    let n = timeout(Duration::from_secs(30), stream.read(&mut buf))
        .await
        .context("Timed out waiting for callback data")?
        .context("Failed to read callback request")?;
    let request = from_utf8(&buf[..n]).context("Invalid UTF-8 in callback request")?;

    match extract_pkce_code(request, expected_state) {
        Ok(code) => Ok((code, stream)),
        Err(e) => {
            // Redirect the browser to the error page before surfacing the
            // error, otherwise the user sees a blank "connection reset" tab.
            // Mirrors the token-exchange error path in the caller.
            redirect_browser(&mut stream, &format!("{app_url}/cli-auth/error")).await;
            Err(e)
        }
    }
}

/// Send a 302 redirect to the browser (best-effort, ignore errors)
async fn redirect_browser(stream: &mut TcpStream, url: &str) {
    let response = format!("HTTP/1.1 302 Found\r\nLocation: {url}\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(response.as_bytes()).await;
}

fn extract_pkce_code(request: &str, expected_state: &str) -> Result<String> {
    // First line of the HTTP request: "GET /callback?code=...&state=... HTTP/1.1"
    let first_line = request.lines().next().context("Empty HTTP request")?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .context("Malformed HTTP request line")?;

    let query = path.split_once('?').map_or("", |(_, q)| q);
    let params = parse_query_string(query);

    let state = params.get("state").context("No state in callback URL")?;
    if state != expected_state {
        bail!("State mismatch in callback — possible CSRF attempt");
    }

    // RFC 6749 §4.1.2.1: the provider may redirect to the callback with
    // `error=...&error_description=...` instead of `code=...` (e.g. when the
    // user denies authorization). Surface that directly.
    if let Some(err) = params.get("error") {
        match params.get("error_description") {
            Some(desc) => bail!("OAuth authorization failed: {err} — {desc}"),
            None => bail!("OAuth authorization failed: {err}"),
        }
    }

    params
        .get("code")
        .cloned()
        .context("No code in callback URL")
}

fn parse_query_string(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            let decoded_v = percent_decode_str(v)
                .decode_utf8()
                .map_or_else(|_| v.to_owned(), Cow::into_owned);
            Some((k.to_owned(), decoded_v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_extracts_code_and_state() {
        let params = parse_query_string("code=abc123&state=xyz");
        assert_eq!(params.get("code").map(String::as_str), Some("abc123"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn parse_query_decodes_percent_encoded_values() {
        let params = parse_query_string("code=hello%3Aworld&state=xyz");
        assert_eq!(params.get("code").map(String::as_str), Some("hello:world"));
    }

    #[test]
    fn parse_query_handles_empty_string() {
        let params = parse_query_string("");
        assert!(params.is_empty());
    }

    #[test]
    fn extract_code_succeeds_with_valid_request() {
        let request =
            "GET /callback?code=testcode&state=teststate HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let code = extract_pkce_code(request, "teststate").unwrap();
        assert_eq!(code, "testcode");
    }

    #[test]
    fn extract_code_rejects_state_mismatch() {
        let request = "GET /callback?code=testcode&state=wrong HTTP/1.1\r\n\r\n";
        let result = extract_pkce_code(request, "expected");
        assert!(result.is_err());
    }

    #[test]
    fn extract_code_rejects_missing_code() {
        let request = "GET /callback?state=teststate HTTP/1.1\r\n\r\n";
        let result = extract_pkce_code(request, "teststate");
        assert!(result.is_err());
    }

    #[test]
    fn extract_code_surfaces_oauth_error_with_description() {
        let request = "GET /callback?error=access_denied&error_description=The%20user%20denied%20the%20request&state=teststate HTTP/1.1\r\n\r\n";
        let err = extract_pkce_code(request, "teststate").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("access_denied"), "got: {msg}");
        assert!(msg.contains("The user denied the request"), "got: {msg}");
    }

    #[test]
    fn extract_code_surfaces_oauth_error_without_description() {
        let request = "GET /callback?error=server_error&state=teststate HTTP/1.1\r\n\r\n";
        let err = extract_pkce_code(request, "teststate").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("server_error"), "got: {msg}");
        // The "no code in callback URL" message is the wrong one for this case —
        // guard against regressing back to it.
        assert!(!msg.contains("No code"), "got: {msg}");
    }

    #[test]
    fn extract_code_still_checks_state_before_oauth_error() {
        // State mismatch must take priority even when an OAuth error is present,
        // so a malicious callback can't avoid the CSRF check by adding `error=`.
        let request = "GET /callback?error=access_denied&state=wrong HTTP/1.1\r\n\r\n";
        let err = extract_pkce_code(request, "expected").unwrap_err();
        assert!(err.to_string().contains("State mismatch"));
    }
}
