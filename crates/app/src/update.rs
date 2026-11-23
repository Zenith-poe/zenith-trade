//! Self-update through Velopack: a check of the GitHub releases at every start, and a new
//! version found then is downloaded and put in place at once, the program starting again by
//! itself. While it runs, a check every 6 h downloads what is new; Velopack puts it in place at
//! the next start, or the tray menu does it at once. A check that fails (no release yet, no
//! network, a program that was not installed by the installer) leaves one line in the log and
//! nothing else.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager, VelopackAsset};

const REPO: &str = "https://github.com/Zenith-poe/zenith-trade";
const MIN_GAP: u64 = 6 * 3600;
const FIRST_CHECK: Duration = Duration::from_secs(2);

/// The update that is downloaded and waits for a restart.
static READY: Mutex<Option<VelopackAsset>> = Mutex::new(None);
static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn manager() -> Result<UpdateManager, velopack::Error> {
    UpdateManager::new(GithubSource::new(REPO, None, false), None, None)
}

fn stamp() -> std::path::PathBuf {
    crate::config::path().with_file_name("update-check.txt")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Whether the last check is old enough for another one.
fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| now.saturating_sub(last) >= MIN_GAP)
}

fn last_check() -> Option<u64> {
    std::fs::read_to_string(stamp()).ok()?.trim().parse().ok()
}

/// The version of the downloaded update, if there is one.
pub fn ready_version() -> Option<String> {
    READY.lock().ok()?.as_ref().map(|asset| asset.Version.clone())
}

fn check_and_download() {
    if BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    match manager().and_then(|m| {
        // Only an installed copy counts as checked: a copy run from elsewhere shares the folder.
        let _ = std::fs::write(stamp(), now().to_string());
        if let Some(pending) = m.get_update_pending_restart() {
            return Ok(Some(pending));
        }
        match m.check_for_updates()? {
            UpdateCheck::UpdateAvailable(info) => {
                log!("update: {} is available, downloading", info.TargetFullRelease.Version);
                m.download_updates(&info, None)?;
                Ok(Some(info.TargetFullRelease))
            }
            _ => Ok(None),
        }
    }) {
        Ok(Some(asset)) => {
            log!("update: {} is ready, it applies when the player restarts", asset.Version);
            if let Ok(mut ready) = READY.lock() {
                *ready = Some(asset);
            }
        }
        Ok(None) => log!("update: up to date"),
        Err(error) => log!("update: no check, {error}"),
    }
    BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// The player asked: checks now, whatever the last check was.
pub fn check_now() {
    let _ = std::thread::Builder::new().name("update-now".into()).spawn(check_and_download);
}

/// Checks right after the start, and puts a new version in place at once while the panel is
/// hidden; then every 6 h while the program runs, never more than once in 6 h across starts.
pub fn spawn() {
    let _ = std::thread::Builder::new().name("update".into()).spawn(|| {
        std::thread::sleep(FIRST_CHECK);
        check_and_download();
        if ready_version().is_some() && !crate::VISIBLE.load(std::sync::atomic::Ordering::SeqCst) {
            log!("update: starting the new version");
            restart();
        }
        loop {
            std::thread::sleep(Duration::from_secs(MIN_GAP));
            if due(last_check(), now()) {
                check_and_download();
            }
        }
    });
}

/// Applies the downloaded update and starts the new version. Does not return when it works.
pub fn restart() {
    let Some(asset) = READY.lock().ok().and_then(|ready| ready.clone()) else { return };
    crate::tray::remove();
    match manager().and_then(|m| m.apply_updates_and_restart(&asset)) {
        Ok(()) => {}
        Err(error) => log!("update: the restart failed, {error}"),
    }
}
