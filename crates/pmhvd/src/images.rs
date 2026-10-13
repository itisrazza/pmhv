//! Storage groups: named directories holding disk images and ISOs.

use std::collections::BTreeMap;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use pmhv_model::{ImageFormat, ImageGroup, ImageInfo, ImageRef};
use serde::Deserialize;
use tokio::process::Command;

pub const DEFAULT_GROUP: &str = "default";

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("no storage group named {0:?}")]
    UnknownGroup(String),
    #[error("{0:?} isn't a valid image name (no slashes, and it can't start with a dot)")]
    InvalidName(String),
    #[error("image {group}/{name} doesn't exist")]
    NotFound { group: String, name: String },
    #[error("image {group}/{name} already exists")]
    AlreadyExists { group: String, name: String },
    #[error("{name} isn't a recognised image type (use .qcow2, .img, .raw or .iso)")]
    UnknownFormat { name: String },
    #[error("failed to read {path}")]
    Io { path: PathBuf, source: io::Error },
    #[error("failed to run {program}")]
    Spawn { program: PathBuf, source: io::Error },
    #[error("{program} failed: {stderr}")]
    Tool { program: PathBuf, stderr: String },
    #[error("{group}/{name} is attached to {}; detach it first", .machines.join(", "))]
    InUse {
        group: String,
        name: String,
        machines: Vec<String>,
    },
    #[error(
        "{name} is {current} bytes; it can only grow (shrinking would cut off the guest's data)"
    )]
    WouldShrink { name: String, current: u64 },
    #[error("ISO images can't be resized")]
    ResizeIso,
    #[error("upload failed")]
    Upload(#[source] axum::Error),
    #[error("unexpected output from qemu-img info for {path}")]
    Info {
        path: PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone)]
pub struct ImageStore {
    groups: BTreeMap<String, PathBuf>,
    qemu_img: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct QemuImgInfo {
    virtual_size: u64,
    actual_size: u64,
}

impl ImageStore {
    pub fn new(data_dir: &Path, configured: &BTreeMap<String, PathBuf>, qemu_img: PathBuf) -> Self {
        let mut groups = BTreeMap::from([(
            DEFAULT_GROUP.to_owned(),
            data_dir.join("images").join(DEFAULT_GROUP),
        )]);
        groups.extend(configured.clone());
        Self { groups, qemu_img }
    }

    /// The path an image would have, after checking the group and name are acceptable.
    pub fn path(&self, image: &ImageRef) -> Result<PathBuf, ImageError> {
        let dir = self
            .groups
            .get(&image.group)
            .ok_or_else(|| ImageError::UnknownGroup(image.group.clone()))?;
        validate_name(&image.name)?;
        Ok(dir.join(&image.name))
    }

    /// The path of an existing image and its format.
    pub fn resolve(&self, image: &ImageRef) -> Result<(PathBuf, ImageFormat), ImageError> {
        let path = self.path(image)?;
        if !path.is_file() {
            return Err(ImageError::NotFound {
                group: image.group.clone(),
                name: image.name.clone(),
            });
        }
        let format = format_of(&image.name).ok_or_else(|| ImageError::UnknownFormat {
            name: image.name.clone(),
        })?;
        Ok((path, format))
    }

    pub async fn list(&self) -> Result<Vec<ImageGroup>, ImageError> {
        let mut groups = Vec::new();
        for (name, dir) in &self.groups {
            groups.push(ImageGroup {
                name: name.clone(),
                images: self.list_group(name, dir).await?,
            });
        }
        Ok(groups)
    }

    async fn list_group(&self, group: &str, dir: &Path) -> Result<Vec<ImageInfo>, ImageError> {
        let io_error = |source| ImageError::Io {
            path: dir.to_owned(),
            source,
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(error)),
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()
            .map_err(io_error)?;
        names.sort();

        let mut images = Vec::new();
        for name in names {
            let Some(format) = format_of(&name) else {
                continue;
            };
            if validate_name(&name).is_err() {
                continue;
            }
            let (allocated_bytes, virtual_bytes) = self.sizes(&dir.join(&name), format).await?;
            images.push(ImageInfo {
                group: group.to_owned(),
                name,
                format,
                allocated_bytes,
                virtual_bytes,
                used_by: Vec::new(),
            });
        }
        Ok(images)
    }

    async fn sizes(&self, path: &Path, format: ImageFormat) -> Result<(u64, u64), ImageError> {
        if format == ImageFormat::Qcow2 {
            let output = self
                .run_qemu_img(&["info", "--output=json", "-U"], path)
                .await?;
            let info: QemuImgInfo =
                serde_json::from_slice(&output).map_err(|source| ImageError::Info {
                    path: path.to_owned(),
                    source,
                })?;
            return Ok((info.actual_size, info.virtual_size));
        }
        let metadata = std::fs::metadata(path).map_err(|source| ImageError::Io {
            path: path.to_owned(),
            source,
        })?;
        Ok((metadata.blocks() * 512, metadata.len()))
    }

    /// Creates an empty image: thin qcow2 for `.qcow2` names, sparse raw for `.img`/`.raw`.
    pub async fn create_blank(&self, image: &ImageRef, size_gib: u32) -> Result<(), ImageError> {
        let path = self.path(image)?;
        let format = match format_of(&image.name) {
            Some(ImageFormat::Qcow2) => "qcow2",
            Some(ImageFormat::Raw) => "raw",
            Some(ImageFormat::Iso) | None => {
                return Err(ImageError::UnknownFormat {
                    name: image.name.clone(),
                });
            }
        };
        if path.exists() {
            return Err(ImageError::AlreadyExists {
                group: image.group.clone(),
                name: image.name.clone(),
            });
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| ImageError::Io {
                path: dir.to_owned(),
                source,
            })?;
        }
        let size = format!("{size_gib}G");
        let mut command = Command::new(&self.qemu_img);
        command
            .args(["create", "-q", "-f", format])
            .arg(&path)
            .arg(size);
        self.run(command).await.map(|_| ())
    }

    /// Streams an upload into the group, refusing to replace an existing image. The data lands in
    /// a hidden file first and is renamed into place once complete, so a broken upload never
    /// looks like an image.
    pub async fn upload(
        &self,
        image: &ImageRef,
        body: impl futures_util::Stream<Item = Result<bytes::Bytes, axum::Error>>,
    ) -> Result<(), ImageError> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let path = self.path(image)?;
        format_of(&image.name).ok_or_else(|| ImageError::UnknownFormat {
            name: image.name.clone(),
        })?;
        let exists = || ImageError::AlreadyExists {
            group: image.group.clone(),
            name: image.name.clone(),
        };
        if path.exists() {
            return Err(exists());
        }
        let dir = path.parent().unwrap_or(Path::new("."));
        let io_error = |path: &Path| {
            let path = path.to_owned();
            move |source| ImageError::Io { path, source }
        };
        std::fs::create_dir_all(dir).map_err(io_error(dir))?;
        let partial = dir.join(format!(".{}.upload", image.name));
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(io_error(&partial))?;

        let mut body = std::pin::pin!(body);
        let written = async {
            while let Some(chunk) = body.next().await {
                file.write_all(&chunk.map_err(ImageError::Upload)?)
                    .await
                    .map_err(io_error(&partial))?;
            }
            file.sync_all().await.map_err(io_error(&partial))
        }
        .await;
        let renamed = written.and_then(|()| {
            if path.exists() {
                return Err(exists());
            }
            std::fs::rename(&partial, &path).map_err(io_error(&path))
        });
        if renamed.is_err() {
            // Best effort: the hidden partial file is skipped by listings either way.
            let _ = std::fs::remove_file(&partial);
        }
        renamed
    }

    pub fn delete(&self, image: &ImageRef) -> Result<(), ImageError> {
        let (path, _) = self.resolve(image)?;
        std::fs::remove_file(&path).map_err(|source| ImageError::Io { path, source })
    }

    pub async fn resize(&self, image: &ImageRef, size_gib: u32) -> Result<(), ImageError> {
        let (path, format) = self.resolve(image)?;
        let format_name = match format {
            ImageFormat::Qcow2 => "qcow2",
            ImageFormat::Raw => "raw",
            ImageFormat::Iso => return Err(ImageError::ResizeIso),
        };
        let (_, current) = self.sizes(&path, format).await?;
        let requested = u64::from(size_gib) << 30;
        if requested < current {
            return Err(ImageError::WouldShrink {
                name: image.name.clone(),
                current,
            });
        }
        let mut command = Command::new(&self.qemu_img);
        command
            .args(["resize", "-q", "-f", format_name])
            .arg(&path)
            .arg(format!("{size_gib}G"));
        self.run(command).await.map(|_| ())
    }

    async fn run_qemu_img(&self, args: &[&str], path: &Path) -> Result<Vec<u8>, ImageError> {
        let mut command = Command::new(&self.qemu_img);
        command.args(args).arg(path);
        self.run(command).await
    }

    async fn run(&self, mut command: Command) -> Result<Vec<u8>, ImageError> {
        let output = command.output().await.map_err(|source| ImageError::Spawn {
            program: self.qemu_img.clone(),
            source,
        })?;
        if !output.status.success() {
            return Err(ImageError::Tool {
                program: self.qemu_img.clone(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(output.stdout)
    }
}

pub fn format_of(name: &str) -> Option<ImageFormat> {
    let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "qcow2" => Some(ImageFormat::Qcow2),
        "img" | "raw" => Some(ImageFormat::Raw),
        "iso" => Some(ImageFormat::Iso),
        _ => None,
    }
}

fn validate_name(name: &str) -> Result<(), ImageError> {
    let valid = !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0']);
    if valid {
        Ok(())
    } else {
        Err(ImageError::InvalidName(name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_formats_by_extension() {
        let cases = [
            ("disk.qcow2", Some(ImageFormat::Qcow2)),
            ("DISK.QCOW2", Some(ImageFormat::Qcow2)),
            ("raw.img", Some(ImageFormat::Raw)),
            ("raw.raw", Some(ImageFormat::Raw)),
            ("debian-13.7.0-arm64-netinst.iso", Some(ImageFormat::Iso)),
            ("notes.txt", None),
            ("no-extension", None),
        ];
        for (name, expected) in cases {
            assert_eq!(format_of(name), expected, "{name}");
        }
    }

    #[test]
    fn validates_image_names() {
        let cases = [
            ("DAVID (AI Agent) - Disk 0.qcow2", true),
            ("archlinux-2026.10.01-x86_64.iso", true),
            ("", false),
            (".hidden.qcow2", false),
            ("../escape.qcow2", false),
            ("dir/disk.qcow2", false),
            ("back\\slash.qcow2", false),
            ("nul\0.qcow2", false),
        ];
        for (name, valid) in cases {
            assert_eq!(validate_name(name).is_ok(), valid, "{name:?}");
        }
    }

    #[test]
    fn confines_paths_to_known_groups() {
        let data = tempfile::tempdir().expect("temp dir");
        let store = ImageStore::new(data.path(), &BTreeMap::new(), PathBuf::from("qemu-img"));
        let image = |group: &str, name: &str| ImageRef {
            group: group.to_owned(),
            name: name.to_owned(),
        };

        let path = store
            .path(&image(DEFAULT_GROUP, "disk.qcow2"))
            .expect("default group");
        assert_eq!(path, data.path().join("images/default/disk.qcow2"));
        assert!(matches!(
            store.path(&image("elsewhere", "disk.qcow2")),
            Err(ImageError::UnknownGroup(_))
        ));
        assert!(matches!(
            store.path(&image(DEFAULT_GROUP, "../../etc/passwd")),
            Err(ImageError::InvalidName(_))
        ));
        assert!(matches!(
            store.resolve(&image(DEFAULT_GROUP, "missing.qcow2")),
            Err(ImageError::NotFound { .. })
        ));
    }
}
