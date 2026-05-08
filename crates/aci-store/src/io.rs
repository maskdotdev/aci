use aci_core::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_json_atomic_with_sync(path, value, true)
}

pub(crate) fn write_json_atomic_unsynced<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_json_atomic_with_sync(path, value, false)
}

fn write_json_atomic_with_sync<T: Serialize>(path: &Path, value: &T, sync: bool) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer(&mut file, value)?;
        writeln!(file)?;
        if sync {
            file.sync_all()?;
        }
    }
    fs::rename(tmp, path)?;
    Ok(())
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
