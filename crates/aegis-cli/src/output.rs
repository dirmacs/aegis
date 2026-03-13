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

#[allow(dead_code)]
impl Styles {
    /// Print a success message with green checkmark prefix.
    pub fn print_success(&self, msg: &str) {
        println!("{} {msg}", self.success.apply_to("✓"));
    }

    /// Print an error message with red X prefix.
    pub fn print_error(&self, msg: &str) {
        println!("{} {msg}", self.error.apply_to("✗"));
    }

    /// Print a warning message with yellow ! prefix.
    pub fn print_warning(&self, msg: &str) {
        println!("{} {msg}", self.warning.apply_to("!"));
    }

    /// Print an info message with cyan ▸ prefix.
    pub fn print_info(&self, msg: &str) {
        println!("{} {msg}", self.info.apply_to("▸"));
    }

    /// Print a section header in bold.
    pub fn print_header(&self, msg: &str) {
        println!("{}", self.bold.apply_to(msg));
    }
}
