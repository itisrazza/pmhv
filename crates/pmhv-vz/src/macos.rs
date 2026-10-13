use std::os::fd::{IntoRawFd, OwnedFd};
use std::path::Path;
use std::sync::{Mutex, PoisonError, mpsc};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSError, NSFileHandle, NSString, NSURL};
use objc2_virtualization::{
    VZDiskImageStorageDeviceAttachment, VZEFIBootLoader, VZEFIVariableStore,
    VZEFIVariableStoreInitializationOptions, VZEntropyDeviceConfiguration,
    VZFileHandleSerialPortAttachment, VZGenericPlatformConfiguration, VZNATNetworkDeviceAttachment,
    VZNVMExpressControllerDeviceConfiguration, VZNetworkDeviceConfiguration,
    VZSerialPortConfiguration, VZStorageDeviceConfiguration, VZUSBMassStorageDeviceConfiguration,
    VZVirtioBlockDeviceConfiguration, VZVirtioConsoleDeviceSerialPortConfiguration,
    VZVirtioEntropyDeviceConfiguration, VZVirtioNetworkDeviceConfiguration, VZVirtualMachine,
    VZVirtualMachineConfiguration, VZVirtualMachineState,
};

use crate::launch::{VzBoot, VzDisk, VzDiskKind, VzLaunch};

#[derive(Debug, Clone, thiserror::Error)]
pub enum VzError {
    #[error("Virtualization.framework isn't available on this Mac")]
    Unsupported,
    #[error("{context}: {message}")]
    Framework {
        context: &'static str,
        message: String,
    },
    #[error("the machine hasn't been created")]
    NoMachine,
    #[error("the main queue stopped responding")]
    MainQueue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VzState {
    Stopped,
    Running,
    Paused,
    Error,
    Starting,
    Pausing,
    Resuming,
    Stopping,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VzAction {
    Start,
    /// Asks the guest to shut down, like pressing the power button.
    RequestStop,
    /// Stops immediately.
    Stop,
    Pause,
    Resume,
}

/// The machine, which belongs to the main queue. `dispatch_main` exits the main thread and GCD
/// then drains the main queue on worker threads, so this can't be a thread-local; it's only
/// ever accessed from blocks on the (serial) main queue.
static MACHINE: Mutex<MainQueueOnly> = Mutex::new(MainQueueOnly(None));

struct MainQueueOnly(Option<Retained<VZVirtualMachine>>);

// SAFETY: the machine is only touched from the main queue, which runs one block at a time, so
// it's never used from two threads at once.
unsafe impl Send for MainQueueOnly {}

/// Runs the main dispatch queue forever. Call this from the main thread once everything else
/// has been started on other threads.
pub fn run_main_queue() -> ! {
    dispatch2::dispatch_main()
}

/// A thread-safe handle to the machine living on the main queue.
#[derive(Debug, Clone, Copy)]
pub struct Vz;

impl Vz {
    /// Builds and validates the configuration and creates the machine, on the main queue.
    /// `serial` becomes the guest's console (hvc0 for Linux).
    pub fn create(launch: &VzLaunch, serial: OwnedFd) -> Result<Self, VzError> {
        let launch = launch.clone();
        let serial = serial.into_raw_fd();
        on_main(move || {
            // SAFETY: we're on the main queue, which `initWithConfiguration` binds the machine to.
            unsafe {
                if !VZVirtualMachine::isSupported() {
                    return Err(VzError::Unsupported);
                }
                let configuration = configuration(&launch, serial)?;
                let machine = VZVirtualMachine::initWithConfiguration(
                    VZVirtualMachine::alloc(),
                    &configuration,
                );
                lock_machine().0 = Some(machine);
                Ok(Self)
            }
        })?
    }

    pub fn state(self) -> Result<VzState, VzError> {
        on_main(|| {
            with_machine(|machine| {
                // SAFETY: on the main queue, which owns the machine.
                let state = unsafe { machine.state() };
                Ok(map_state(state))
            })
        })?
    }

    /// Performs `action`, waiting for the framework's completion handler where it has one.
    pub fn act(self, action: VzAction) -> Result<(), VzError> {
        let (done_tx, done_rx) = mpsc::channel::<Result<(), VzError>>();
        on_main(move || {
            let result = with_machine(|machine| {
                let handler_done = done_tx.clone();
                let handler = RcBlock::new(move |error: *mut NSError| {
                    // SAFETY: the framework passes either null or a valid NSError.
                    let result = match unsafe { error.as_ref() } {
                        None => Ok(()),
                        Some(error) => Err(framework_error(action_name(action), error)),
                    };
                    // The waiting side may have given up; nothing to do then.
                    let _ = handler_done.send(result);
                });
                // SAFETY: on the main queue, which owns the machine.
                unsafe {
                    match action {
                        VzAction::Start => machine.startWithCompletionHandler(&handler),
                        VzAction::Stop => machine.stopWithCompletionHandler(&handler),
                        VzAction::Pause => machine.pauseWithCompletionHandler(&handler),
                        VzAction::Resume => machine.resumeWithCompletionHandler(&handler),
                        VzAction::RequestStop => {
                            let result = machine
                                .requestStopWithError()
                                .map_err(|error| framework_error("request stop", &error));
                            let _ = done_tx.send(result);
                        }
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                let _ = done_tx.send(Err(error));
            }
        })?;
        done_rx.recv().map_err(|_| VzError::MainQueue)?
    }
}

/// Runs `work` on the main queue and waits for its result.
fn on_main<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, VzError> {
    let (result_tx, result_rx) = mpsc::channel();
    DispatchQueue::main().exec_async(move || {
        let _ = result_tx.send(work());
    });
    result_rx.recv().map_err(|_| VzError::MainQueue)
}

fn lock_machine() -> std::sync::MutexGuard<'static, MainQueueOnly> {
    MACHINE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Call on the main queue.
fn with_machine<T>(
    work: impl FnOnce(&VZVirtualMachine) -> Result<T, VzError>,
) -> Result<T, VzError> {
    match lock_machine().0.as_deref() {
        Some(machine) => work(machine),
        None => Err(VzError::NoMachine),
    }
}

fn action_name(action: VzAction) -> &'static str {
    match action {
        VzAction::Start => "start",
        VzAction::RequestStop => "request stop",
        VzAction::Stop => "stop",
        VzAction::Pause => "pause",
        VzAction::Resume => "resume",
    }
}

fn framework_error(context: &'static str, error: &NSError) -> VzError {
    VzError::Framework {
        context,
        message: error.localizedDescription().to_string(),
    }
}

fn map_state(state: VZVirtualMachineState) -> VzState {
    match state {
        VZVirtualMachineState::Stopped => VzState::Stopped,
        VZVirtualMachineState::Running => VzState::Running,
        VZVirtualMachineState::Paused => VzState::Paused,
        VZVirtualMachineState::Error => VzState::Error,
        VZVirtualMachineState::Starting => VzState::Starting,
        VZVirtualMachineState::Pausing => VzState::Pausing,
        VZVirtualMachineState::Resuming => VzState::Resuming,
        VZVirtualMachineState::Stopping => VzState::Stopping,
        _ => VzState::Other,
    }
}

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// SAFETY: call on the main queue.
unsafe fn configuration(
    launch: &VzLaunch,
    serial: i32,
) -> Result<Retained<VZVirtualMachineConfiguration>, VzError> {
    unsafe {
        let configuration = VZVirtualMachineConfiguration::new();
        configuration.setCPUCount(usize::from(launch.cpu_cores));
        configuration.setMemorySize(u64::from(launch.memory_mib) << 20);

        let VzBoot::Efi { variable_store } = &launch.boot;
        let boot_loader = VZEFIBootLoader::new();
        let store = efi_variable_store(variable_store)?;
        boot_loader.setVariableStore(Some(&store));
        configuration.setBootLoader(Some(&boot_loader));
        configuration.setPlatform(&VZGenericPlatformConfiguration::new());

        let storage = launch
            .disks
            .iter()
            .map(|disk| storage_device(disk))
            .collect::<Result<Vec<_>, _>>()?;
        configuration.setStorageDevices(&NSArray::from_retained_slice(&storage));

        if launch.nat {
            let network = VZVirtioNetworkDeviceConfiguration::new();
            network.setAttachment(Some(&VZNATNetworkDeviceAttachment::new()));
            let network: Retained<VZNetworkDeviceConfiguration> = Retained::into_super(network);
            configuration.setNetworkDevices(&NSArray::from_retained_slice(&[network]));
        }

        let read = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
            NSFileHandle::alloc(),
            serial,
            false,
        );
        let write = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
            NSFileHandle::alloc(),
            serial,
            true,
        );
        let attachment =
            VZFileHandleSerialPortAttachment::initWithFileHandleForReading_fileHandleForWriting(
                VZFileHandleSerialPortAttachment::alloc(),
                Some(&read),
                Some(&write),
            );
        let port = VZVirtioConsoleDeviceSerialPortConfiguration::new();
        port.setAttachment(Some(&attachment));
        let port: Retained<VZSerialPortConfiguration> = Retained::into_super(port);
        configuration.setSerialPorts(&NSArray::from_retained_slice(&[port]));

        let entropy: Retained<VZEntropyDeviceConfiguration> =
            Retained::into_super(VZVirtioEntropyDeviceConfiguration::new());
        configuration.setEntropyDevices(&NSArray::from_retained_slice(&[entropy]));

        configuration
            .validateWithError()
            .map_err(|error| framework_error("invalid configuration", &error))?;
        Ok(configuration)
    }
}

/// SAFETY: call on the main queue.
unsafe fn efi_variable_store(path: &Path) -> Result<Retained<VZEFIVariableStore>, VzError> {
    let url = file_url(path);
    unsafe {
        if path.exists() {
            return Ok(VZEFIVariableStore::initWithURL(
                VZEFIVariableStore::alloc(),
                &url,
            ));
        }
        VZEFIVariableStore::initCreatingVariableStoreAtURL_options_error(
            VZEFIVariableStore::alloc(),
            &url,
            VZEFIVariableStoreInitializationOptions::empty(),
        )
        .map_err(|error| framework_error("creating the EFI variable store", &error))
    }
}

/// SAFETY: call on the main queue.
unsafe fn storage_device(disk: &VzDisk) -> Result<Retained<VZStorageDeviceConfiguration>, VzError> {
    unsafe {
        let attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
            VZDiskImageStorageDeviceAttachment::alloc(),
            &file_url(&disk.path),
            disk.read_only,
        )
        .map_err(|error| framework_error("attaching a disk image", &error))?;
        Ok(match disk.kind {
            VzDiskKind::Virtio => {
                Retained::into_super(VZVirtioBlockDeviceConfiguration::initWithAttachment(
                    VZVirtioBlockDeviceConfiguration::alloc(),
                    &attachment,
                ))
            }
            VzDiskKind::Nvme => Retained::into_super(
                VZNVMExpressControllerDeviceConfiguration::initWithAttachment(
                    VZNVMExpressControllerDeviceConfiguration::alloc(),
                    &attachment,
                ),
            ),
            VzDiskKind::Usb => {
                Retained::into_super(VZUSBMassStorageDeviceConfiguration::initWithAttachment(
                    VZUSBMassStorageDeviceConfiguration::alloc(),
                    &attachment,
                ))
            }
        })
    }
}
