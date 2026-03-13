use console::Style;

/// Standard output styles used across commands.
#[allow(dead_code)]
pub struct Styles {
    pub success: Style,
    pub error: Style,
    pub warning: Style,
    pub info: Style,
    pub bold: Style,
    pub dim: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            success: Style::new().green().bold(),
            error: Style::new().red().bold(),
            warning: Style::new().yellow(),
            info: Style::new().cyan(),
            bold: Style::new().bold(),
            dim: Style::new().dim(),
        }
    }
}
