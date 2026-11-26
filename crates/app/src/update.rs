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

/// Where the update stands, for the quick menu and the Settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Unknown,
    Checking,
    UpToDate,
    /// Downloaded, this version applies at the restart.
    Ready(String),
    Failed,
    /// A copy that Velopack did not install: nothing to update.
    NotInstalled,
}

static STATE: Mutex<State> = Mutex::new(State::Unknown);

pub fn state() -> State {
    STATE.lock().map_or(State::Unknown, |s| s.clone())
}

fn set_state(new: State) {
    if let Ok(mut s) = STATE.lock() {
        if *s == new {
            return;
        }
        *s = new;
    }
    if let Some(ctx) = crate::CTX.get() {
        ctx.request_repaint();
    }
}

/// The line shown: the version and where the update stands.
pub fn status_text() -> String {
    let version = env!("CARGO_PKG_VERSION");
    match state() {
        State::Unknown => format!("v{version}"),
        State::Checking => format!("v{version} · Checking…"),
        State::UpToDate => format!("v{version} · Up to date"),
        State::Ready(new) => format!("v{version} · {new} ready"),
        State::Failed => format!("v{version} · Check failed"),
        State::NotInstalled => format!("v{version} · dev copy"),
    }
}

/// The small action next to the line, if there is one: its label.
pub fn action_label() -> Option<&'static str> {
    match state() {
        State::Ready(_) => Some("Restart"),
        State::Unknown | State::UpToDate | State::Failed => Some("Check"),
        State::Checking | State::NotInstalled => None,
    }
}

/// The action of the line: restarts onto the ready version, or checks now.
pub fn run_action() {
    match state() {
        State::Ready(_) => restart_to_update(),
        State::Unknown | State::UpToDate | State::Failed => check_now(),
        State::Checking | State::NotInstalled => {}
    }
}

/// Applies the downloaded update and restarts, off the UI thread.
pub fn restart_to_update() {
    let _ = std::thread::Builder::new().name("update-restart".into()).spawn(restart);
}

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
    set_state(State::Checking);
    let manager = manager();
    if manager.is_err() {
        set_state(State::NotInstalled);
    }
    match manager.and_then(|m| {
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
            let version = asset.Version.clone();
            if let Ok(mut ready) = READY.lock() {
                *ready = Some(asset);
            }
            set_state(State::Ready(version));
        }
        Ok(None) => {
            log!("update: up to date");
            set_state(State::UpToDate);
        }
        Err(error) => {
            log!("update: no check, {error}");
            if state() != State::NotInstalled {
                set_state(State::Failed);
            }
        }
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
