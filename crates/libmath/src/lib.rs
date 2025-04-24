use autotrait::autotrait;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    &ModImpl
}

/// The mode for LaTeX/MathML rendering
pub enum MathMode {
    Inline,
    Block,
}

/// A simple plugin that renders math/LaTeX-ish markup to MathML
#[autotrait]
impl Mod for ModImpl {
    /// Render math markup with pulldown-latex
    fn render_math(
        &self,
        input: &str,
        mode: MathMode,
        w: &mut dyn std::io::Write,
    ) -> eyre::Result<()> {
        let storage = pulldown_latex::Storage::new();
        let parser = pulldown_latex::Parser::new(input, &storage);
        let mut config = pulldown_latex::RenderConfig::default();
        config.display_mode = match mode {
            MathMode::Inline => pulldown_latex::config::DisplayMode::Inline,
            MathMode::Block => pulldown_latex::config::DisplayMode::Block,
        };

        let mut rendered = String::new();
        pulldown_latex::push_mathml(&mut rendered, parser, config)?;
        w.write_all(rendered.as_bytes())?;

        Ok(())
    }
}
