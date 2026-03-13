use std::path::PathBuf;

use anyhow::{Result, bail};

use super::Context;

#[derive(clap::Args)]
pub struct InitArgs {
    /// Directory to initialize (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,
}

pub async fn run(args: InitArgs, _ctx: &Context) -> Result<()> {
    let dir = PathBuf::from(&args.path);
    let manifest_path = dir.join("aegis.toml");

    if manifest_path.exists() {
        bail!("aegis.toml already exists at {}", manifest_path.display());
    }

    let template = r#"[aegis]
version = "0.1.0"
description = ""
default_profile = "dev-vps"
strategy = "symlink"

[variables]
hostname = { source = "command", value = "hostname" }
user = { source = "env", value = "USER" }

[profiles.dev-vps]
description = "Development VPS with full tooling"
modules = ["shell", "terminal", "dev-tools", "ai-tools", "dirmacs"]

[profiles.ci]
description = "Minimal CI environment"
modules = ["shell", "dev-tools"]

[[modules]]
name = "shell"
path = "modules/shell"

[[modules]]
name = "terminal"
path = "modules/terminal"

[[modules]]
name = "ai-tools"
path = "modules/ai-tools"

[[modules]]
name = "dev-tools"
path = "modules/dev-tools"

[[modules]]
name = "dirmacs"
path = "modules/dirmacs"
"#;

    // Create module directories
    let module_dirs = ["shell", "terminal", "ai-tools", "dev-tools", "dirmacs"];
    for name in &module_dirs {
        let module_dir = dir.join("modules").join(name);
        std::fs::create_dir_all(&module_dir)?;

        let module_toml = module_dir.join("module.toml");
        if !module_toml.exists() {
            let content = format!(
                r#"[module]
name = "{name}"
description = ""
"#
            );
            std::fs::write(&module_toml, content)?;
        }
    }

    std::fs::write(&manifest_path, template)?;

    let style = console::Style::new().green().bold();
    println!(
        "{} Initialized aegis at {}",
        style.apply_to("✓"),
        manifest_path.display()
    );
    println!("  Created module directories under modules/");
    println!("  Edit aegis.toml and module.toml files to configure your system");

    Ok(())
}
