use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use tera::Tera;
use tracing::debug;

/// Render a template file with the given variables.
pub fn render_file(
    source: &Path,
    variables: &HashMap<String, String>,
) -> Result<String> {
    let content = std::fs::read_to_string(source)
        .with_context(|| format!("reading template {}", source.display()))?;
    render_string(&content, variables)
        .with_context(|| format!("rendering template {}", source.display()))
}

/// Render a template string with the given variables.
pub fn render_string(
    template: &str,
    variables: &HashMap<String, String>,
) -> Result<String> {
    let mut tera = Tera::default();
    tera.add_raw_template("__inline__", template)
        .context("compiling template")?;

    let mut ctx = tera::Context::new();
    for (key, value) in variables {
        ctx.insert(key, value);
    }

    debug!("rendering template with {} variables", variables.len());
    let rendered = tera.render("__inline__", &ctx).context("rendering template")?;
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_simple_template() {
        let vars = HashMap::from([
            ("name".to_string(), "aegis".to_string()),
            ("version".to_string(), "0.1.0".to_string()),
        ]);
        let result = render_string("Hello {{ name }} v{{ version }}", &vars).unwrap();
        assert_eq!(result, "Hello aegis v0.1.0");
    }

    #[test]
    fn render_with_conditionals() {
        let vars = HashMap::from([("gpu_available".to_string(), "true".to_string())]);
        let tmpl = "{% if gpu_available == \"true\" %}GPU{% else %}CPU{% endif %}";
        let result = render_string(tmpl, &vars).unwrap();
        assert_eq!(result, "GPU");
    }
}
