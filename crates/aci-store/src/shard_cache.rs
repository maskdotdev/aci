use aci_core::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const SHARD_COUNT: usize = 256;
const FLUSH_THRESHOLD_BYTES: usize = 64 * 1024;

pub(crate) struct ShardWriterCache {
    root: PathBuf,
    extension: &'static str,
    buffers: Vec<Vec<u8>>,
}

impl ShardWriterCache {
    pub(crate) fn new(root: PathBuf, extension: &'static str) -> Self {
        Self {
            root,
            extension,
            buffers: vec![Vec::new(); SHARD_COUNT],
        }
    }

    pub(crate) fn write_all(&mut self, shard: u8, bytes: &[u8]) -> Result<()> {
        let buffer = &mut self.buffers[usize::from(shard)];
        buffer.extend_from_slice(bytes);
        if buffer.len() >= FLUSH_THRESHOLD_BYTES {
            self.flush_shard(shard)?;
        }
        Ok(())
    }

    pub(crate) fn flush(mut self) -> Result<()> {
        for shard in 0..SHARD_COUNT {
            self.flush_shard(shard as u8)?;
        }
        Ok(())
    }

    fn flush_shard(&mut self, shard: u8) -> Result<()> {
        let buffer = &mut self.buffers[usize::from(shard)];
        if buffer.is_empty() {
            return Ok(());
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join(format!("{shard:02x}.{}", self.extension)))?;
        file.write_all(buffer)?;
        buffer.clear();
        Ok(())
    }
}
