use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use console::Style;

use aegis_opencode::oh_my_opencode::{self, OhMyOpencodeInput};
use aegis_opencode::opencode::{self, OpencodeInput};

use super::Context;

#[derive(clap::Args)]
pub struct OpencodeArgs {
    #[command(subcommand)]
    pub command: OpencodeCommand,
}

#[derive(clap::Subcommand)]
pub enum OpencodeCommand {
    /// Generate opencode.json and oh-my-opencode.json from TOML definitions
    Generate(GenerateArgs),
    /// Validate the TOML definitions without writing files
    Validate(ValidateArgs),
}

#[derive(clap::Args)]
pub struct GenerateArgs {
    /// Path to the opencode TOML file (defaults to modules/ai-tools/opencode.toml)
    #[arg(long)]
    pub input: Option<String>,
    /// Output directory for generated JSON files
    #[arg(long)]
    pub output: Option<String>,
}

#[derive(clap::Args)]
pub struct ValidateArgs {
    /// Path to the opencode TOML file
    #[arg(long)]
    pub input: Option<String>,
}

pub async fn run(args: OpencodeArgs, ctx: &Context) -> Result<()> {
    match args.command {
        OpencodeCommand::Generate(gen_args) => generate(gen_args, ctx).await,
        OpencodeCommand::Validate(val_args) => validate(val_args, ctx).await,
    }
}

async fn generate(args: GenerateArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();

    let input_path = resolve_input_path(args.input.as_deref(), ctx)?;
    let output_dir = if let Some(ref out) = args.output {
        PathBuf::from(out)
    } else {
        // Default to ~/.config/opencode/ or wherever opencode expects its config
        dirs::config_dir()
            .map(|d: std::path::PathBuf| d.join("opencode"))
            .unwrap_or_else(|| PathBuf::from("."))
    };

    // Load and parse the TOML
    let content = std::fs::read_to_string(&input_path)
        .with_context(|| format!("reading {}", input_path.display()))?;

    // Parse opencode section
    let opencode_input = OpencodeInput::load(&input_path)
        .context("parsing opencode config")?;

    // Generate opencode.json
    let opencode_json = opencode_input.generate()?;
    let opencode_str = opencode::to_json_string(&opencode_json)?;

    let opencode_out = output_dir.join("opencode.json");
    if ctx.dry_run {
        println!("[dry-run] would write {}", opencode_out.display());
        println!("{opencode_str}");
    } else {
        std::fs::create_dir_all(&output_dir)?;
        std::fs::write(&opencode_out, &opencode_str)?;
        println!(
            "{} Generated {}",
            green.apply_to("✓"),
            opencode_out.display()
        );
    }

    // Parse and generate oh-my-opencode section if present
    if content.contains("[oh_my_opencode") {
        let omoc_input = OhMyOpencodeInput::load(&input_path)
            .context("parsing oh-my-opencode config")?;
        let omoc_json = omoc_input.generate(&opencode_input.models)?;
        let omoc_str = oh_my_opencode::to_json_string(&omoc_json)?;

        let omoc_out = output_dir.join("oh-my-opencode.json");
        if ctx.dry_run {
            println!("[dry-run] would write {}", omoc_out.display());
            println!("{omoc_str}");
        } else {
            std::fs::write(&omoc_out, &omoc_str)?;
            println!(
                "{} Generated {}",
                green.apply_to("✓"),
                omoc_out.display()
            );
        }
    }

    Ok(())
}

async fn validate(args: ValidateArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let input_path = resolve_input_path(args.input.as_deref(), ctx)?;

    let content = std::fs::read_to_string(&input_path)
        .with_context(|| format!("reading {}", input_path.display()))?;

    // Try parsing opencode
    let opencode_input = OpencodeInput::load(&input_path)?;

    // Try generating to catch reference errors
    let _opencode_json = opencode_input.generate()?;

    // Try oh-my-opencode if present
    if content.contains("[oh_my_opencode") {
        let omoc_input = OhMyOpencodeInput::load(&input_path)?;
        let _omoc_json = omoc_input.generate(&opencode_input.models)?;
    }

    println!(
        "{} Valid opencode configuration at {}",
        green.apply_to("✓"),
        input_path.display()
    );
    Ok(())
}

/// Resolve the opencode TOML input path.
fn resolve_input_path(explicit: Option<&str>, ctx: &Context) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(PathBuf::from(path));
    }

    // Try to find it relative to the manifest
    if let Ok((_, manifest_dir)) = ctx.load_manifest() {
        let candidate = manifest_dir.join("modules/ai-tools/opencode.toml");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    // Fall back to current directory
    let candidate = PathBuf::from("opencode.toml");
    if candidate.exists() {
        return Ok(candidate);
    }

    bail!("no opencode.toml found — specify one with --input");
}
