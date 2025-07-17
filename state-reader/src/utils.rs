use std::{
    fs::{self, File},
    path::Path,
    thread,
    time::Duration,
};

use lockfile::Lockfile;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::StateReaderError;

pub fn read_atomically<P: AsRef<Path>, D: DeserializeOwned>(
    path: P,
) -> Result<D, StateReaderError> {
    let lock_path = path.as_ref().with_extension("lock");

    let mut lockfile = Lockfile::create_with_parents(&lock_path);
    while let Err(lockfile::Error::LockTaken) = lockfile {
        thread::sleep(Duration::from_secs(1));
        lockfile = Lockfile::create_with_parents(&lock_path);
    }
    let lockfile = lockfile?;

    let file = File::open(path)?;
    let value = serde_json::from_reader(file)?;

    lockfile.release()?;

    Ok(value)
}

pub fn write_atomically<P: AsRef<Path>, S: Serialize>(
    path: P,
    value: S,
) -> Result<(), StateReaderError> {
    let lock_path = path.as_ref().with_extension("lock");
    let tmp_path = path.as_ref().with_extension("tmp");

    let mut lockfile = Lockfile::create_with_parents(&lock_path);
    while let Err(lockfile::Error::LockTaken) = lockfile {
        thread::sleep(Duration::from_secs(1));
        lockfile = Lockfile::create_with_parents(&lock_path);
    }
    let lockfile = lockfile?;

    let file = File::create(&tmp_path)?;
    serde_json::to_writer(file, &value)?;
    fs::rename(tmp_path, path)?;

    lockfile.release()?;

    Ok(())
}

pub fn merge_atomically<P: AsRef<Path>, S: Serialize + DeserializeOwned>(
    path: P,
    value: S,
    merger: impl FnOnce(S, S) -> S,
) -> Result<(), StateReaderError> {
    let lock_path = path.as_ref().with_extension("lock");
    let tmp_path = path.as_ref().with_extension("tmp");

    let mut lockfile = Lockfile::create_with_parents(&lock_path);
    while let Err(lockfile::Error::LockTaken) = lockfile {
        thread::sleep(Duration::from_secs(1));
        lockfile = Lockfile::create_with_parents(&lock_path);
    }
    let lockfile = lockfile?;

    let tmp_file = File::create(&tmp_path)?;
    if let Ok(file) = File::open(&path) {
        let cached_value = serde_json::from_reader(file)?;
        let merged_value = merger(cached_value, value);

        serde_json::to_writer(tmp_file, &merged_value)?;
    } else {
        serde_json::to_writer(tmp_file, &value)?;
    }

    fs::rename(tmp_path, path)?;

    lockfile.release()?;

    Ok(())
}
