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

    /// Creates an empty qcow2 image.
    pub async fn create_blank(&self, image: &ImageRef, size_gib: u32) -> Result<(), ImageError> {
        let path = self.path(image)?;
        if format_of(&image.name) != Some(ImageFormat::Qcow2) {
            return Err(ImageError::UnknownFormat {
                name: image.name.clone(),
            });
        }
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
            .args(["create", "-q", "-f", "qcow2"])
            .arg(&path)
            .arg(size);
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
