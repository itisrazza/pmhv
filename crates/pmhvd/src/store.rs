//! Machine specs on disk: `<data dir>/machines/<id>/machine.toml`.

use std::io;
use std::path::{Path, PathBuf};

use pmhv_model::{MachineId, MachineSpec};
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("failed to read {path}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid machine spec {path}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to serialise machine spec")]
    Serialise(#[from] toml::ser::Error),
    #[error("failed to write {path}")]
    Write { path: PathBuf, source: io::Error },
    #[error("failed to delete {path}")]
    Delete { path: PathBuf, source: io::Error },
}

#[derive(Debug, Clone)]
pub struct MachineStore {
    root: PathBuf,
}

impl MachineStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("machines"),
        }
    }

    pub fn machine_dir(&self, id: MachineId) -> PathBuf {
        self.root.join(id.to_string())
    }

    pub fn nvram_file(&self, id: MachineId) -> PathBuf {
        self.machine_dir(id).join("nvram.fd")
    }

    pub fn tpm_dir(&self, id: MachineId) -> PathBuf {
        self.machine_dir(id).join("tpm")
    }

    pub fn shim_log(&self, id: MachineId) -> PathBuf {
        self.machine_dir(id).join("shim.log")
    }

    fn spec_file(&self, id: MachineId) -> PathBuf {
        self.machine_dir(id).join("machine.toml")
    }

    /// Loads every machine. Directories that aren't machines are skipped with a warning.
    pub fn load_all(&self) -> Result<Vec<(MachineId, MachineSpec)>, StoreError> {
        let read_error = |source| StoreError::Read {
            path: self.root.clone(),
            source,
        };
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(read_error(error)),
        };
        let mut machines = Vec::new();
        for entry in entries {
            let entry = entry.map_err(read_error)?;
            let name = entry.file_name();
            let Some(id) = name
                .to_str()
                .and_then(|name| name.parse::<MachineId>().ok())
            else {
                warn!(path = %entry.path().display(), "skipping non-machine directory");
                continue;
            };
            machines.push((id, self.load(id)?));
        }
        Ok(machines)
    }

    fn load(&self, id: MachineId) -> Result<MachineSpec, StoreError> {
        let path = self.spec_file(id);
        let text = std::fs::read_to_string(&path).map_err(|source| StoreError::Read {
            path: path.clone(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| StoreError::Parse { path, source })
    }

    /// Writes the spec via a temporary file so a crash never leaves a half-written one.
    pub fn save(&self, id: MachineId, spec: &MachineSpec) -> Result<(), StoreError> {
        let dir = self.machine_dir(id);
        std::fs::create_dir_all(&dir).map_err(|source| StoreError::Write {
            path: dir.clone(),
            source,
        })?;
        let path = self.spec_file(id);
        let temporary = path.with_extension("toml.tmp");
        let text = toml::to_string_pretty(spec)?;
        std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, &path))
            .map_err(|source| StoreError::Write { path, source })
    }

    /// Deletes the machine's directory: spec, firmware variables, TPM state and logs.
    /// Disk images live in the image store and are left alone.
    pub fn delete(&self, id: MachineId) -> Result<(), StoreError> {
        let dir = self.machine_dir(id);
        std::fs::remove_dir_all(&dir).map_err(|source| StoreError::Delete { path: dir, source })
    }
}
