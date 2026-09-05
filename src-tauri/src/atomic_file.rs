//! Durable replacement without exposing partial JSON to concurrent readers.
use std::{fs::OpenOptions, io::Write, path::Path};

pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut random = [0u8; 16];
    getrandom::getrandom(&mut random)
        .map_err(|_| std::io::Error::other("Randomness unavailable"))?;
    let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let temporary = parent.join(format!(".ember-write-{suffix}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
