//! Runs an Apple Virtualization.framework machine. The machine itself lives on the main queue
//! (see `main`); calls into it block until the main queue answers, so they run on tokio's
//! blocking pool.

use std::time::Duration;

use pmhv_driver::ShimPower;
use pmhv_model::MachineState;
use pmhv_vz::{Vz, VzAction, VzError, VzLaunch, VzState};
use tracing::info;

use crate::machine::{ControlError, Machine, RunError};

/// How often to check the machine's state: the framework reports it, but only when asked.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, VzError> + Send + 'static,
) -> Result<Result<T, VzError>, tokio::task::JoinError> {
    tokio::task::spawn_blocking(work).await
}

impl Machine {
    pub(crate) async fn run_vz(&self, launch: &VzLaunch) -> Result<Option<String>, RunError> {
        let (ours, theirs) =
            std::os::unix::net::UnixStream::pair().map_err(RunError::SerialPair)?;
        let launch = launch.clone();
        let vz = blocking(move || Vz::create(&launch, theirs.into()))
            .await
            .map_err(|_| RunError::VzPanicked)??;

        ours.set_nonblocking(true).map_err(RunError::SerialPair)?;
        let ours = tokio::net::UnixStream::from_std(ours).map_err(RunError::SerialPair)?;
        self.console.attach(ours);

        blocking(move || vz.act(VzAction::Start))
            .await
            .map_err(|_| RunError::VzPanicked)??;
        // `set` only fails if already set, which can't happen: this is the only place it's set.
        let _ = self.vz.set(vz);
        self.set_state(MachineState::Running);
        info!("machine running");

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let state = blocking(move || vz.state())
                .await
                .map_err(|_| RunError::VzPanicked)??;
            match state {
                VzState::Stopped => return Ok(None),
                VzState::Error => {
                    return Ok(Some(
                        "the virtual machine stopped because of an error".to_owned(),
                    ));
                }
                VzState::Running | VzState::Resuming => self.set_state(MachineState::Running),
                VzState::Paused | VzState::Pausing => self.set_state(MachineState::Paused),
                VzState::Stopping => self.set_state(MachineState::Stopping),
                VzState::Starting | VzState::Other => {}
            }
        }
    }
}

pub(crate) async fn power(vz: Vz, action: ShimPower) -> Result<(), ControlError> {
    let action = match action {
        ShimPower::Stop => VzAction::RequestStop,
        ShimPower::ForceStop => VzAction::Stop,
        ShimPower::Pause => VzAction::Pause,
        ShimPower::Resume => VzAction::Resume,
        ShimPower::Reset => return Err(ControlError::VzReset),
    };
    blocking(move || vz.act(action))
        .await
        .map_err(|_| ControlError::VzPanicked)??;
    Ok(())
}
