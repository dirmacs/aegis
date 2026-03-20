//! Enforcement — compare current system state against the manifest and produce
//! a remediation plan to bring the system into compliance.

use std::path::PathBuf;

use anyhow::Result;

use crate::config::{ConfigMapping, deploy_config};
use crate::manifest::{LinkStrategy, Manifest};
use crate::module::Module;
use crate::package::PackageStatus;

/// A single remediation action.
#[derive(Debug)]
pub enum Action {
    InstallPackage {
        module: String,
        package: String,
        method: String,
    },
    DeployConfig {
        module: String,
        source: PathBuf,
        target: PathBuf,
        strategy: LinkStrategy,
    },
    RepairDrift {
        module: String,
        source: PathBuf,
        target: PathBuf,
        strategy: LinkStrategy,
    },
}

/// The full remediation plan.
#[derive(Debug)]
pub struct RemediationPlan {
    pub actions: Vec<Action>,
}

impl RemediationPlan {
    pub fn is_clean(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn summary(&self) -> (usize, usize, usize) {
        let installs = self.actions.iter().filter(|a| matches!(a, Action::InstallPackage { .. })).count();
        let deploys = self.actions.iter().filter(|a| matches!(a, Action::DeployConfig { .. })).count();
        let repairs = self.actions.iter().filter(|a| matches!(a, Action::RepairDrift { .. })).count();
        (installs, deploys, repairs)
    }
}

/// Build a remediation plan by comparing modules against current system state.
pub fn plan_remediation(
    modules: &[Module],
    default_strategy: LinkStrategy,
) -> Result<RemediationPlan> {
    let mut actions = Vec::new();

    for module in modules {
        // Check packages
        for pkg in &module.manifest.packages {
            let status: PackageStatus = pkg.check_status();
            if !status.installed {
                actions.push(Action::InstallPackage {
                    module: module.name.clone(),
                    package: pkg.name.clone(),
                    method: format!("{:?}", pkg.install_method),
                });
            }
        }

        // Check configs
        for cfg in &module.manifest.configs {
            let source = module.config_source_path(cfg);
            let target = module.config_target_path(cfg)?;
            let strategy = module.effective_strategy(cfg, default_strategy);

            if !target.exists() {
                actions.push(Action::DeployConfig {
                    module: module.name.clone(),
                    source,
                    target,
                    strategy,
                });
            } else if strategy != LinkStrategy::Symlink {
                // Check for drift (content mismatch)
                if let (Ok(src_content), Ok(tgt_content)) = (
                    std::fs::read_to_string(&source),
                    std::fs::read_to_string(&target),
                ) {
                    if src_content != tgt_content {
                        actions.push(Action::RepairDrift {
                            module: module.name.clone(),
                            source,
                            target,
                            strategy,
                        });
                    }
                }
            }
        }
    }

    Ok(RemediationPlan { actions })
}

/// Apply the remediation plan.
pub fn apply_remediation(
    plan: &RemediationPlan,
    modules: &[Module],
    dry_run: bool,
) -> Result<(usize, usize)> {
    let mut ok = 0usize;
    let mut fail = 0usize;

    for action in &plan.actions {
        match action {
            Action::InstallPackage { module: mod_name, package: pkg_name, .. } => {
                // Find the package spec from the module
                let pkg = modules
                    .iter()
                    .find(|m| m.name == *mod_name)
                    .and_then(|m| m.manifest.packages.iter().find(|p| p.name == *pkg_name));

                if let Some(pkg) = pkg {
                    match pkg.install(dry_run) {
                        Ok(()) => ok += 1,
                        Err(e) => {
                            tracing::error!("failed to install {pkg_name}: {e}");
                            fail += 1;
                        }
                    }
                }
            }
            Action::DeployConfig { source, target, strategy, .. }
            | Action::RepairDrift { source, target, strategy, .. } => {
                match deploy_config(source, target, *strategy, dry_run) {
                    Ok(()) => ok += 1,
                    Err(e) => {
                        tracing::error!("failed to deploy {}: {e}", target.display());
                        fail += 1;
                    }
                }
            }
        }
    }

    Ok((ok, fail))
}
