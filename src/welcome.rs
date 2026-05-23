use std::io;

pub const ART: &str = r"
 ██████╗ ██████╗ ██████╗ ███████╗██╗  ██╗
██╔════╝██╔═══██╗██╔══██╗██╔════╝╚██╗██╔╝
██║     ██║   ██║██║  ██║█████╗   ╚███╔╝
██║     ██║   ██║██║  ██║██╔══╝   ██╔██╗
╚██████╗╚██████╔╝██████╔╝███████╗██╔╝ ██╗
 ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝";

pub const TAGLINE: &str = "Terminal-first ebook library and ereader manager.";

pub const HINT: &str = "Run `cdx --help` to see available commands.";

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn render_plain<W: io::Write>(w: &mut W) -> io::Result<()> {
    writeln!(w, "{ART}")?;
    writeln!(w, "  codex v{}", version())?;
    writeln!(w, "  {TAGLINE}")?;
    writeln!(w)?;
    writeln!(w, "  {HINT}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        let mut buf = Vec::new();
        render_plain(&mut buf).expect("render_plain writes to in-memory buffer; cannot fail");
        String::from_utf8(buf).expect("render_plain emits valid UTF-8")
    }

    #[test]
    fn render_plain_includes_version() {
        let out = rendered();
        assert!(out.contains(&format!("codex v{}", version())));
    }

    #[test]
    fn render_plain_includes_tagline() {
        assert!(rendered().contains(TAGLINE));
    }

    #[test]
    fn render_plain_includes_help_hint() {
        assert!(rendered().contains(HINT));
    }

    #[test]
    fn render_plain_includes_art() {
        assert!(rendered().contains("██████╗"));
    }
}
