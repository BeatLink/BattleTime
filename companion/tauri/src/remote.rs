// Runs a remote button's shell command on this computer; the command comes only from the user's own button settings, never from the watch.

use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

// ============================================================================
// Limits
// ============================================================================

// How long a command may run before it is stopped.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

// The most output kept from a command, from its end, since only a status line shows it.
const OUTPUT_LIMIT: usize = 4000;

// ============================================================================
// Running a command
// ============================================================================

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    // None when the command ran out of time or was ended by a signal.
    pub exit_code: Option<i32>,
    pub output: String,
    pub timed_out: bool,
}

pub async fn run_shell_command(command: &str, limit: Duration) -> Result<CommandResult, String> {
    if command.trim().is_empty() {
        return Err("Give a command to run.".into());
    }
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Could not start the command: {error}"))?;

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let finished = tokio::time::timeout(limit, async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (status, _, _) = tokio::join!(
            child.wait(),
            stdout.read_to_end(&mut out),
            stderr.read_to_end(&mut err)
        );
        out.extend_from_slice(&err);
        (status, out)
    })
    .await;

    Ok(match finished {
        Ok((status, output)) => CommandResult {
            exit_code: status.ok().and_then(|status| status.code()),
            output: tail(&String::from_utf8_lossy(&output), OUTPUT_LIMIT),
            timed_out: false,
        },
        Err(_) => {
            let _ = child.kill().await;
            CommandResult {
                exit_code: None,
                output: String::new(),
                timed_out: true,
            }
        }
    })
}

// The last `limit` characters of `text`.
fn tail(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(limit)).collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_command_reports_its_exit_code_and_output() {
        let result = run_shell_command("echo out; echo err >&2; exit 3", COMMAND_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(
            result,
            CommandResult {
                exit_code: Some(3),
                output: "out\nerr\n".into(),
                timed_out: false
            }
        );
    }

    #[tokio::test]
    async fn a_command_that_runs_too_long_is_stopped() {
        let result = run_shell_command("sleep 5", Duration::from_millis(100))
            .await
            .unwrap();
        assert!(result.timed_out);
        assert_eq!(result.exit_code, None);
    }

    #[tokio::test]
    async fn an_empty_command_is_refused() {
        assert_eq!(
            run_shell_command("  ", COMMAND_TIMEOUT).await,
            Err("Give a command to run.".into())
        );
    }

    #[test]
    fn long_output_keeps_its_end() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("ab", 3), "ab");
    }
}
