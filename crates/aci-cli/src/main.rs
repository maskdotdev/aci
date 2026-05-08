use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    aci_cli::Cli::parse().run()
}
