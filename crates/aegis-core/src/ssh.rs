use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, Result, bail};
use tracing::info;

/// SSH target for remote command execution.
#[derive(Debug, Clone)]
pub struct SshTarget {
    pub user: String,
    pub host: String,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

impl SshTarget {
    /// Build the base SSH command with common flags.
    fn ssh_cmd(&self) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.args(["-o", "BatchMode=yes"]);
        cmd.args(["-o", "ConnectTimeout=10"]);
        cmd.args(["-o", "StrictHostKeyChecking=accept-new"]);
        if let Some(port) = self.port {
            cmd.args(["-p", &port.to_string()]);
        }
        if let Some(ref key) = self.identity_file {
            cmd.args(["-i", key]);
        }
        cmd.arg(format!("{}@{}", self.user, self.host));
        cmd
    }

    /// The user@host string for display.
    pub fn display(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    /// Run a command on the remote host, capturing stdout.
    pub fn exec(&self, command: &str) -> Result<String> {
        let mut cmd = self.ssh_cmd();
        cmd.arg(command);
        cmd.stderr(Stdio::inherit());

        let output = cmd
            .output()
            .with_context(|| format!("ssh to {}", self.display()))?;

        if !output.status.success() {
            bail!(
                "remote command failed on {} (exit {}): {}",
                self.display(),
                output.status.code().unwrap_or(-1),
                command,
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Run a command on the remote host, streaming stdout/stderr to the terminal.
    pub fn exec_interactive(&self, command: &str) -> Result<ExitStatus> {
        let mut cmd = self.ssh_cmd();
        cmd.arg(command);
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let status = cmd
            .status()
            .with_context(|| format!("ssh to {}", self.display()))?;

        Ok(status)
    }

    /// Copy a local file to the remote host via scp.
    pub fn push_file(&self, local: &Path, remote: &str) -> Result<()> {
        info!("scp {} → {}:{}", local.display(), self.display(), remote);

        let mut cmd = Command::new("scp");
        cmd.args(["-o", "BatchMode=yes"]);
        cmd.args(["-o", "ConnectTimeout=10"]);
        if let Some(port) = self.port {
            cmd.args(["-P", &port.to_string()]);
        }
        if let Some(ref key) = self.identity_file {
            cmd.args(["-i", key]);
        }
        cmd.arg(local.to_string_lossy().as_ref());
        cmd.arg(format!("{}@{}:{}", self.user, self.host, remote));

        let status = cmd
            .status()
            .with_context(|| format!("scp to {}", self.display()))?;

        if !status.success() {
            bail!("scp to {} failed", self.display());
        }
        Ok(())
    }

    /// Copy a remote file to a local path via scp.
    pub fn pull_file(&self, remote: &str, local: &Path) -> Result<()> {
        info!("scp {}:{} → {}", self.display(), remote, local.display());

        let mut cmd = Command::new("scp");
        cmd.args(["-o", "BatchMode=yes"]);
        cmd.args(["-o", "ConnectTimeout=10"]);
        if let Some(port) = self.port {
            cmd.args(["-P", &port.to_string()]);
        }
        if let Some(ref key) = self.identity_file {
            cmd.args(["-i", key]);
        }
        cmd.arg(format!("{}@{}:{}", self.user, self.host, remote));
        cmd.arg(local.to_string_lossy().as_ref());

        let status = cmd
            .status()
            .with_context(|| format!("scp from {}", self.display()))?;

        if !status.success() {
            bail!("scp from {} failed", self.display());
        }
        Ok(())
    }

    /// Rsync a local directory to the remote host.
    pub fn rsync_push(&self, local: &Path, remote: &str, excludes: &[&str]) -> Result<()> {
        info!("rsync {} → {}:{}", local.display(), self.display(), remote);

        let mut cmd = Command::new("rsync");
        cmd.args(["-avz", "--delete"]);

        // SSH transport options
        let mut ssh_opts = String::from("ssh -o BatchMode=yes -o ConnectTimeout=10");
        if let Some(port) = self.port {
            ssh_opts.push_str(&format!(" -p {port}"));
        }
        if let Some(ref key) = self.identity_file {
            ssh_opts.push_str(&format!(" -i {key}"));
        }
        cmd.args(["-e", &ssh_opts]);

        for exclude in excludes {
            cmd.args(["--exclude", exclude]);
        }

        // Ensure trailing slash for directory sync
        let local_str = format!("{}/", local.display());
        cmd.arg(&local_str);
        cmd.arg(format!("{}@{}:{}", self.user, self.host, remote));

        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let status = cmd
            .status()
            .with_context(|| format!("rsync to {}", self.display()))?;

        if !status.success() {
            bail!("rsync to {} failed", self.display());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format() {
        let target = SshTarget {
            user: "root".into(),
            host: "10.42.0.1".into(),
            port: None,
            identity_file: None,
        };
        assert_eq!(target.display(), "root@10.42.0.1");
    }

    #[test]
    fn display_format_with_port() {
        let target = SshTarget {
            user: "user".into(),
            host: "10.42.0.2".into(),
            port: Some(2222),
            identity_file: None,
        };
        assert_eq!(target.display(), "user@10.42.0.2");
    }
}
