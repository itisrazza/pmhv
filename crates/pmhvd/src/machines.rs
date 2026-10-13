//! The machine registry: specs from the store, live state from shims, and the events both produce.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pmhv_driver::{
    MachineRuntimeDir, Request, Response, ShimPower, ShimStatus, read_message, write_message,
};
use pmhv_model::report::error_chain;
use pmhv_model::{
    Arch, ConsoleKind, CreateMachineRequest, DiskMedia, DiskSpec, Event, ImageRef, Machine,
    MachineId, MachineSpec, MachineState, PowerAction, Thumbnail, ThumbnailKind,
};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::{broadcast, oneshot};
use tracing::{debug, info, warn};

use crate::config::QemuConfig;
use crate::images::{DEFAULT_GROUP, ImageError, ImageStore};
use crate::resolve::{ResolveError, Resolver};
use crate::store::{MachineStore, StoreError};
use crate::thumbnails;

/// How long a freshly spawned shim has to open its control socket.
const SHIM_STARTUP: Duration = Duration::from_secs(15);

/// Long enough after starting for there to usually be something on screen.
const FIRST_THUMBNAIL_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("no machine with id {0}")]
    NotFound(MachineId),
    #[error("{0}")]
    Conflict(String),
    #[error("invalid machine spec")]
    Invalid(#[source] ResolveError),
    #[error("can't start the machine")]
    Start(#[source] ResolveError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(
        "pmhv-shim not found at {0}; build the whole workspace (`cargo build --workspace`) or set `shim` in config.toml"
    )]
    ShimNotFound(PathBuf),
    #[error("failed to prepare {path}")]
    Prepare { path: PathBuf, source: io::Error },
    #[error("failed to start pmhv-shim")]
    Spawn(#[source] io::Error),
    #[error("can't reach the machine's shim at {path}")]
    ShimConnect { path: PathBuf, source: io::Error },
    #[error("talking to the machine's shim failed")]
    ShimProtocol(#[from] pmhv_driver::MessageError),
    #[error("the shim closed the connection without replying")]
    ShimNoReply,
    #[error("{0}")]
    Shim(String),
}

pub struct Context {
    pub store: MachineStore,
    pub images: ImageStore,
    pub qemu: QemuConfig,
    pub runtime_dir: PathBuf,
    pub shim: PathBuf,
    pub thumbnail_interval: Duration,
}

struct Entry {
    spec: MachineSpec,
    state: MachineState,
    last_error: Option<String>,
    thumbnail: Option<Thumbnail>,
    /// Bumped each time the machine starts, so a previous run's background tasks can tell
    /// they're stale.
    run: u64,
}

pub struct Machines {
    entries: Mutex<HashMap<MachineId, Entry>>,
    events: broadcast::Sender<Event>,
    ctx: Context,
}

impl Machines {
    /// Loads stored machines and reattaches to any shims still running from a previous pmhvd.
    pub async fn load(ctx: Context) -> Result<Arc<Self>, MachineError> {
        let entries = ctx
            .store
            .load_all()?
            .into_iter()
            .map(|(id, spec)| {
                let thumbnail = thumbnails::current(&ctx.store.machine_dir(id));
                let entry = Entry {
                    spec,
                    state: MachineState::Stopped,
                    last_error: None,
                    thumbnail: thumbnail.map(|(thumbnail, _)| thumbnail),
                    run: 0,
                };
                (id, entry)
            })
            .collect();
        let (events, _) = broadcast::channel(256);
        let machines = Arc::new(Self {
            entries: Mutex::new(entries),
            events,
            ctx,
        });
        machines.reattach().await;
        Ok(machines)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub fn images(&self) -> &ImageStore {
        &self.ctx.images
    }

    pub fn list(&self) -> Vec<Machine> {
        let entries = self.lock();
        let mut machines: Vec<Machine> =
            entries.iter().map(|(id, entry)| view(*id, entry)).collect();
        machines.sort_by_key(|machine| machine.spec.name.to_lowercase());
        machines
    }

    pub fn get(&self, id: MachineId) -> Result<Machine, MachineError> {
        self.lock()
            .get(&id)
            .map(|entry| view(id, entry))
            .ok_or(MachineError::NotFound(id))
    }

    pub async fn create(&self, request: CreateMachineRequest) -> Result<Machine, MachineError> {
        let mut spec = request.spec;
        self.resolver()
            .check(&spec)
            .map_err(MachineError::Invalid)?;
        if let Some(disk) = request.new_boot_disk {
            let image = ImageRef {
                group: DEFAULT_GROUP.to_owned(),
                name: format!("{} - Disk 0.qcow2", spec.name.replace(['/', '\\'], "-")),
            };
            self.ctx.images.create_blank(&image, disk.size_gib).await?;
            spec.disks.insert(
                0,
                DiskSpec {
                    image,
                    bus: disk.bus,
                    media: DiskMedia::Disk,
                    read_only: false,
                },
            );
        }

        let id = MachineId::generate();
        self.ctx.store.save(id, &spec)?;
        let entry = Entry {
            spec,
            state: MachineState::Stopped,
            last_error: None,
            thumbnail: None,
            run: 0,
        };
        let machine = view(id, &entry);
        self.lock().insert(id, entry);
        self.emit(Event::MachineUpdated {
            machine: machine.clone(),
        });
        info!(%id, name = machine.spec.name, "machine created");
        Ok(machine)
    }

    pub fn update(&self, id: MachineId, spec: MachineSpec) -> Result<Machine, MachineError> {
        self.resolver()
            .check(&spec)
            .map_err(MachineError::Invalid)?;
        let machine = {
            let mut entries = self.lock();
            let entry = entries.get_mut(&id).ok_or(MachineError::NotFound(id))?;
            require_stopped(entry, "edited")?;
            self.ctx.store.save(id, &spec)?;
            entry.spec = spec;
            view(id, entry)
        };
        self.emit(Event::MachineUpdated {
            machine: machine.clone(),
        });
        Ok(machine)
    }

    pub fn delete(&self, id: MachineId) -> Result<(), MachineError> {
        {
            let mut entries = self.lock();
            let entry = entries.get(&id).ok_or(MachineError::NotFound(id))?;
            require_stopped(entry, "deleted")?;
            self.ctx.store.delete(id)?;
            entries.remove(&id);
        }
        self.emit(Event::MachineRemoved { id });
        info!(%id, "machine deleted");
        Ok(())
    }

    pub async fn power(
        self: &Arc<Self>,
        id: MachineId,
        action: PowerAction,
    ) -> Result<Machine, MachineError> {
        let shim_action = match action {
            PowerAction::Start => {
                self.start(id)?;
                return self.get(id);
            }
            PowerAction::Stop => ShimPower::Stop,
            PowerAction::ForceStop => ShimPower::ForceStop,
            PowerAction::Reset => ShimPower::Reset,
            PowerAction::Pause => ShimPower::Pause,
            PowerAction::Resume => ShimPower::Resume,
        };
        match self.get(id)?.state {
            MachineState::Stopped => {
                return Err(MachineError::Conflict(
                    "the machine isn't running".to_owned(),
                ));
            }
            MachineState::Starting
            | MachineState::Running
            | MachineState::Paused
            | MachineState::Stopping => {}
        }
        match self
            .shim_request(
                id,
                &Request::Power {
                    action: shim_action,
                },
            )
            .await?
        {
            Response::Ok => self.get(id),
            other @ (Response::Status(_) | Response::Scrollback { .. }) => {
                Err(MachineError::Shim(format!("unexpected reply {other:?}")))
            }
            Response::Error { message } => Err(MachineError::Shim(message)),
        }
    }

    /// The shim's serial console socket, for a machine that's running.
    pub fn console_socket(&self, id: MachineId) -> Result<PathBuf, MachineError> {
        self.require_live(id)?;
        Ok(self.runtime_dir(id).console_socket())
    }

    /// QEMU's VNC socket, for a graphical machine that's running.
    pub fn vnc_socket(&self, id: MachineId) -> Result<PathBuf, MachineError> {
        let machine = self.require_live(id)?;
        if machine.spec.console != pmhv_model::ConsoleKind::Graphical {
            return Err(MachineError::Conflict(
                "the machine has a text console".to_owned(),
            ));
        }
        Ok(self.runtime_dir(id).vnc_socket())
    }

    fn require_live(&self, id: MachineId) -> Result<Machine, MachineError> {
        let machine = self.get(id)?;
        if is_live(machine.state) {
            Ok(machine)
        } else {
            Err(MachineError::Conflict(
                "the machine isn't running".to_owned(),
            ))
        }
    }

    fn start(self: &Arc<Self>, id: MachineId) -> Result<(), MachineError> {
        let (spec, run) = {
            let mut entries = self.lock();
            let entry = entries.get_mut(&id).ok_or(MachineError::NotFound(id))?;
            require_stopped(entry, "started")?;
            entry.state = MachineState::Starting;
            entry.last_error = None;
            entry.run += 1;
            (entry.spec.clone(), entry.run)
        };
        self.emit_machine(id);

        match self.spawn_shim(id, &spec) {
            Ok(exited) => {
                tokio::spawn(Arc::clone(self).supervise(id, Some(exited)));
                tokio::spawn(Arc::clone(self).capture_periodically(id, run));
                Ok(())
            }
            Err(error) => {
                self.set_status(id, MachineState::Stopped, Some(error_chain(&error)));
                Err(error)
            }
        }
    }

    /// Starts a shim for the machine, returning a receiver that fires when it exits.
    fn spawn_shim(
        &self,
        id: MachineId,
        spec: &MachineSpec,
    ) -> Result<oneshot::Receiver<()>, MachineError> {
        if !self.ctx.shim.is_file() {
            return Err(MachineError::ShimNotFound(self.ctx.shim.clone()));
        }
        let dir = self.runtime_dir(id);
        prepare_runtime_dir(&dir)?;
        let launch = self
            .resolver()
            .resolve(id, spec, &dir)
            .map_err(MachineError::Start)?;
        let launch_path = dir.launch_file();
        let prepare_error = |source| MachineError::Prepare {
            path: launch_path.clone(),
            source,
        };
        let json =
            serde_json::to_vec_pretty(&launch).map_err(|error| prepare_error(error.into()))?;
        std::fs::write(&launch_path, json).map_err(prepare_error)?;

        let log_path = self.ctx.store.shim_log(id);
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|source| MachineError::Prepare {
                path: log_path.clone(),
                source,
            })?;
        let log_stderr = log.try_clone().map_err(|source| MachineError::Prepare {
            path: log_path.clone(),
            source,
        })?;

        let mut command = Command::new(&self.ctx.shim);
        command
            .arg(&launch_path)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(log_stderr)
            // Its own process group, so a Ctrl+C aimed at pmhvd doesn't take the machine down too.
            .process_group(0);
        let mut child = command.spawn().map_err(MachineError::Spawn)?;
        info!(%id, pid = child.id(), "shim started");

        let (exited_tx, exited_rx) = oneshot::channel();
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => info!(%id, %status, "shim exited"),
                Err(error) => warn!(%id, %error, "waiting for shim failed"),
            }
            // The supervisor may have finished already; nobody needs telling then.
            let _ = exited_tx.send(());
        });
        Ok(exited_rx)
    }

    /// Follows a shim's status until it exits. `exited` is set for shims this pmhvd spawned.
    async fn supervise(self: Arc<Self>, id: MachineId, exited: Option<oneshot::Receiver<()>>) {
        let dir = self.runtime_dir(id);
        let final_status = match self.watch(id, &dir, exited).await {
            Ok(Some(status)) => Some(status),
            Ok(None) => None,
            Err(error) => {
                warn!(%id, error = error_chain(&error), "lost track of shim");
                None
            }
        };
        let status = final_status
            .or_else(|| read_exit_file(&dir))
            .unwrap_or_else(|| ShimStatus {
                state: MachineState::Stopped,
                error: Some(format!(
                    "pmhv-shim exited unexpectedly; see {}",
                    self.ctx.store.shim_log(id).display()
                )),
                exited: true,
            });
        self.set_status(id, MachineState::Stopped, status.error);
        remove_runtime_dir(&dir);
    }

    /// Returns the shim's final status if it sent one before closing.
    async fn watch(
        self: &Arc<Self>,
        id: MachineId,
        dir: &MachineRuntimeDir,
        exited: Option<oneshot::Receiver<()>>,
    ) -> Result<Option<ShimStatus>, MachineError> {
        let Some(stream) = connect_when_ready(&dir.shim_socket(), exited).await? else {
            return Ok(None);
        };
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        write_message(&mut write, &Request::Watch).await?;
        while let Some(response) = read_message::<Response>(&mut read).await? {
            let Response::Status(status) = response else {
                continue;
            };
            if status.exited {
                return Ok(Some(status));
            }
            self.set_status(id, status.state, status.error);
        }
        Ok(None)
    }

    /// Picks up shims left running by a previous pmhvd and clears out dead ones' directories.
    async fn reattach(self: &Arc<Self>) {
        let parent = MachineRuntimeDir::parent(&self.ctx.runtime_dir);
        let Ok(entries) = std::fs::read_dir(&parent) else {
            return;
        };
        for entry in entries.flatten() {
            let dir = MachineRuntimeDir::from_path(entry.path());
            let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<MachineId>().ok())
            else {
                continue;
            };
            if !self.lock().contains_key(&id) {
                warn!(%id, "runtime directory for an unknown machine; leaving it alone");
                continue;
            }
            match self.shim_request(id, &Request::Status).await {
                Ok(Response::Status(status)) if !status.exited => {
                    info!(%id, state = ?status.state, "reattached to running machine");
                    self.set_status(id, status.state, status.error);
                    let run = self.lock().get_mut(&id).map_or(0, |entry| {
                        entry.run += 1;
                        entry.run
                    });
                    tokio::spawn(Arc::clone(self).supervise(id, None));
                    tokio::spawn(Arc::clone(self).capture_periodically(id, run));
                }
                _ => {
                    if let Some(status) = read_exit_file(&dir) {
                        self.set_status(id, MachineState::Stopped, status.error);
                    }
                    remove_runtime_dir(&dir);
                }
            }
        }
    }

    async fn shim_request(
        &self,
        id: MachineId,
        request: &Request,
    ) -> Result<Response, MachineError> {
        let path = self.runtime_dir(id).shim_socket();
        let stream = UnixStream::connect(&path)
            .await
            .map_err(|source| MachineError::ShimConnect { path, source })?;
        let (read, mut write) = stream.into_split();
        write_message(&mut write, request).await?;
        read_message(&mut BufReader::new(read))
            .await?
            .ok_or(MachineError::ShimNoReply)
    }

    fn set_status(self: &Arc<Self>, id: MachineId, state: MachineState, error: Option<String>) {
        let previous = {
            let mut entries = self.lock();
            let Some(entry) = entries.get_mut(&id) else {
                return;
            };
            if entry.state == state && entry.last_error == error {
                return;
            }
            let previous = entry.state;
            entry.state = state;
            entry.last_error = error;
            previous
        };
        self.emit_machine(id);
        // The guest's on its way down: grab its last screen while there's still one to grab.
        if state == MachineState::Stopping && previous != MachineState::Stopping {
            let machines = Arc::clone(self);
            tokio::spawn(async move { machines.capture_thumbnail(id).await });
        }
    }

    /// Captures soon after start, then every `thumbnail_interval`, until this run ends.
    async fn capture_periodically(self: Arc<Self>, id: MachineId, run: u64) {
        tokio::time::sleep(FIRST_THUMBNAIL_AFTER).await;
        loop {
            let current = self.lock().get(&id).map(|entry| (entry.run, entry.state));
            match current {
                Some((current_run, state)) if current_run == run && is_live(state) => {}
                _ => return,
            }
            self.capture_thumbnail(id).await;
            tokio::time::sleep(self.ctx.thumbnail_interval).await;
        }
    }

    /// Captures the machine's screen now, if it's running. Failures are logged, not returned:
    /// a missing thumbnail isn't worth interrupting anything for.
    pub async fn capture_thumbnail(&self, id: MachineId) {
        let Ok(machine) = self.get(id) else {
            return;
        };
        if !is_live(machine.state) {
            return;
        }
        let dir = self.ctx.store.machine_dir(id);
        let captured = match machine.spec.console {
            ConsoleKind::Graphical => self.capture_image(id, &dir).await,
            ConsoleKind::Text => self.capture_text(id, &dir).await,
        };
        let kind = match captured {
            Ok(kind) => kind,
            Err(error) => {
                debug!(%id, error = error_chain(&error), "thumbnail capture failed");
                return;
            }
        };
        if let Some(entry) = self.lock().get_mut(&id) {
            entry.thumbnail = Some(Thumbnail {
                kind,
                captured_at: thumbnails::now_millis(),
            });
        }
        self.emit_machine(id);
    }

    async fn capture_image(
        &self,
        id: MachineId,
        dir: &Path,
    ) -> Result<ThumbnailKind, MachineError> {
        let path = thumbnails::image_file(dir);
        let temporary = path.with_extension("png.tmp");
        let request = Request::Screenshot {
            path: temporary.clone(),
        };
        match self.shim_request(id, &request).await? {
            Response::Ok => {}
            Response::Error { message } => return Err(MachineError::Shim(message)),
            other => return Err(MachineError::Shim(format!("unexpected reply {other:?}"))),
        }
        let io_error = |source| MachineError::Prepare {
            path: path.clone(),
            source,
        };
        std::fs::rename(&temporary, &path).map_err(io_error)?;
        thumbnails::remove_if_present(&thumbnails::text_file(dir)).map_err(io_error)?;
        Ok(ThumbnailKind::Image)
    }

    async fn capture_text(&self, id: MachineId, dir: &Path) -> Result<ThumbnailKind, MachineError> {
        let text = match self.shim_request(id, &Request::Scrollback).await? {
            Response::Scrollback { text } => text,
            Response::Error { message } => return Err(MachineError::Shim(message)),
            other => return Err(MachineError::Shim(format!("unexpected reply {other:?}"))),
        };
        thumbnails::write_text(dir, &text).map_err(|source| MachineError::Prepare {
            path: thumbnails::text_file(dir),
            source,
        })?;
        Ok(ThumbnailKind::Text)
    }

    /// The machine's newest thumbnail file.
    pub fn thumbnail_file(
        &self,
        id: MachineId,
    ) -> Result<Option<(Thumbnail, PathBuf)>, MachineError> {
        self.get(id)?;
        Ok(thumbnails::current(&self.ctx.store.machine_dir(id)))
    }

    fn emit_machine(&self, id: MachineId) {
        if let Ok(machine) = self.get(id) {
            self.emit(Event::MachineUpdated { machine });
        }
    }

    fn emit(&self, event: Event) {
        // No subscribers just means no browser has the event stream open.
        let _ = self.events.send(event);
    }

    fn runtime_dir(&self, id: MachineId) -> MachineRuntimeDir {
        MachineRuntimeDir::new(&self.ctx.runtime_dir, id)
    }

    fn resolver(&self) -> Resolver<'_> {
        Resolver {
            qemu: &self.ctx.qemu,
            store: &self.ctx.store,
            images: &self.ctx.images,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<MachineId, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn view(id: MachineId, entry: &Entry) -> Machine {
    Machine {
        id,
        spec: entry.spec.clone(),
        state: entry.state,
        emulated: Arch::host() != Some(entry.spec.arch),
        last_error: entry.last_error.clone(),
        thumbnail: entry.thumbnail,
    }
}

fn is_live(state: MachineState) -> bool {
    matches!(
        state,
        MachineState::Running | MachineState::Paused | MachineState::Stopping
    )
}

fn require_stopped(entry: &Entry, verb: &str) -> Result<(), MachineError> {
    if entry.state == MachineState::Stopped {
        Ok(())
    } else {
        Err(MachineError::Conflict(format!(
            "the machine must be stopped to be {verb}"
        )))
    }
}

/// Waits for the shim's control socket, giving up if the shim exits or takes too long.
async fn connect_when_ready(
    path: &Path,
    mut exited: Option<oneshot::Receiver<()>>,
) -> Result<Option<UnixStream>, MachineError> {
    let deadline = tokio::time::Instant::now() + SHIM_STARTUP;
    loop {
        match UnixStream::connect(path).await {
            Ok(stream) => return Ok(Some(stream)),
            Err(source) if tokio::time::Instant::now() >= deadline => {
                return Err(MachineError::ShimConnect {
                    path: path.to_owned(),
                    source,
                });
            }
            Err(_) => {}
        }
        if let Some(receiver) = exited.as_mut()
            && receiver.try_recv().is_ok()
        {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn prepare_runtime_dir(dir: &MachineRuntimeDir) -> Result<(), MachineError> {
    remove_runtime_dir(dir);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir.path())
        .map_err(|source| MachineError::Prepare {
            path: dir.path().to_owned(),
            source,
        })
}

fn remove_runtime_dir(dir: &MachineRuntimeDir) {
    match std::fs::remove_dir_all(dir.path()) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            warn!(path = %dir.path().display(), %error, "failed to remove runtime directory");
        }
    }
}

fn read_exit_file(dir: &MachineRuntimeDir) -> Option<ShimStatus> {
    let text = std::fs::read_to_string(dir.exit_file()).ok()?;
    serde_json::from_str(&text)
        .inspect_err(|error| warn!(path = %dir.exit_file().display(), %error, "invalid exit file"))
        .ok()
}
