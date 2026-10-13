//! Manual smoke test: boots Apple's EFI firmware with a blank disk and reports state changes.
//! Must be signed with the virtualization entitlement (see `just sign`).
//!
//! `efi_smoke <scratch dir>`

#[cfg(target_os = "macos")]
fn main() {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::Duration;

    use pmhv_vz::{Vz, VzAction, VzBoot, VzDisk, VzDiskKind, VzLaunch, run_main_queue};

    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: efi_smoke <scratch dir>"),
    );
    let disk = dir.join("disk.img");
    std::fs::File::create(&disk)
        .and_then(|file| file.set_len(1 << 30))
        .expect("create disk");
    let launch = VzLaunch {
        name: "smoke".to_owned(),
        cpu_cores: 2,
        memory_mib: 1024,
        boot: VzBoot::Efi {
            variable_store: dir.join("efi-vars.fd"),
        },
        disks: vec![VzDisk {
            path: disk,
            kind: VzDiskKind::Virtio,
            read_only: false,
        }],
        nat: true,
    };

    std::thread::spawn(move || {
        let (ours, theirs) = UnixStream::pair().expect("socket pair");
        let vz = Vz::create(&launch, theirs.into()).expect("create");
        println!("created; state {:?}", vz.state());
        vz.act(VzAction::Start).expect("start");
        ours.set_read_timeout(Some(Duration::from_millis(200)))
            .expect("timeout");
        let mut ours = ours;
        for _ in 0..15 {
            let mut buffer = [0; 4096];
            let read = ours.read(&mut buffer).unwrap_or(0);
            println!("state {:?}, {read} serial bytes", vz.state());
            std::thread::sleep(Duration::from_millis(200));
        }
        vz.act(VzAction::Pause).expect("pause");
        println!("after pause: {:?}", vz.state());
        vz.act(VzAction::Resume).expect("resume");
        println!("after resume: {:?}", vz.state());
        vz.act(VzAction::Stop).expect("stop");
        println!("after stop: {:?}", vz.state());
        std::process::exit(0);
    });
    run_main_queue();
}

#[cfg(not(target_os = "macos"))]
fn main() {}
