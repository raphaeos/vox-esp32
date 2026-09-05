#![no_std]

use anyhow::Result;

fn main() -> Result<()> {
    let _controller = vox_esp32_core::init()?;

    Ok(())
}
