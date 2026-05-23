#![warn(clippy::all)]
#![deny(unsafe_code)]

fn main() -> anyhow::Result<()> {
    codex::run()
}
