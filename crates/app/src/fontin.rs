//! Fontin SmallCaps, the game's tooltip font, for the item card. It is not shipped: the program
//! fetches it once from the game's CDN (as the PoE Kit site does), keeps the WOFF in the icons'
//! cache folder, and reads it from there on later runs. Until it is there the card falls back to
//! the panel's font.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::time::Duration;

use eframe::egui;

const URL: &str = "https://web.poecdn.com/font/fontin-smallcaps-webfont.woff";
const FILE: &str = "fontin-smallcaps.woff";

/// The decoded font and its ascender and descender, per em.
struct Font {
    sfnt: Vec<u8>,
    metrics: (f32, f32),
}

static FONT: OnceLock<Font> = OnceLock::new();
/// The family is known to egui: `set_fonts` was applied at a frame start.
static LIVE: AtomicBool = AtomicBool::new(false);
/// The pass at which `set_fonts` was called, plus one (0: not yet); it applies at the next pass.
static PENDING: AtomicU64 = AtomicU64::new(0);
static FETCHING: AtomicBool = AtomicBool::new(false);

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A WOFF 1.0 file as the sfnt (TTF or OTF) it wraps; `None` when it is not a sound WOFF.
pub fn woff_to_sfnt(woff: &[u8]) -> Option<Vec<u8>> {
    if woff.get(0..4)? != b"wOFF" {
        return None;
    }
    let flavor = be32(woff, 4)?;
    let n = be16(woff, 12)? as usize;
    if n == 0 || n > 64 {
        return None;
    }
    let mut tables = Vec::with_capacity(n);
    for i in 0..n {
        let e = 44 + i * 20;
        let tag = woff.get(e..e + 4)?;
        let offset = be32(woff, e + 4)? as usize;
        let comp = be32(woff, e + 8)? as usize;
        let orig = be32(woff, e + 12)? as usize;
        let checksum = be32(woff, e + 16)?;
        let stored = woff.get(offset..offset.checked_add(comp)?)?;
        let data = if comp < orig {
            let data = miniz_oxide::inflate::decompress_to_vec_zlib(stored).ok()?;
            if data.len() != orig {
                return None;
            }
            data
        } else if comp == orig {
            stored.to_vec()
        } else {
            return None;
        };
        tables.push((tag, checksum, data));
    }
    let mut entry_selector = 0u16;
    while (1usize << (entry_selector + 1)) <= n {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = n as u16 * 16 - search_range;

    let mut out = Vec::new();
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(n as u16).to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());
    let mut at = 12 + 16 * n;
    for (tag, checksum, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum.to_be_bytes());
        out.extend_from_slice(&(at as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        at += data.len().div_ceil(4) * 4;
    }
    for (_, _, data) in &tables {
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    Some(out)
}

/// Ascender and descender per em, from `hhea` and `head`.
fn metrics(sfnt: &[u8]) -> Option<(f32, f32)> {
    let n = be16(sfnt, 4)? as usize;
    let table = |tag: &[u8; 4]| {
        (0..n).find_map(|i| {
            let e = 12 + i * 16;
            (sfnt.get(e..e + 4)? == tag).then(|| be32(sfnt, e + 8).map(|o| o as usize))?
        })
    };
    let (hhea, head) = (table(b"hhea")?, table(b"head")?);
    let em = be16(sfnt, head + 18)? as f32;
    let asc = be16(sfnt, hhea + 4)? as i16 as f32;
    let desc = be16(sfnt, hhea + 6)? as i16 as f32;
    (em > 0.0).then_some((asc / em, -desc / em))
}

fn set(woff: &[u8]) -> bool {
    let Some(sfnt) = woff_to_sfnt(woff) else { return false };
    let Some(metrics) = metrics(&sfnt) else { return false };
    FONT.set(Font { sfnt, metrics }).is_ok()
}

fn cache_file() -> Option<std::path::PathBuf> {
    Some(crate::panel::icons::cache_dir()?.join(FILE))
}

/// Reads the cached font, if there is one, on this thread: it is 29 KB. Call it before the fonts
/// are installed, and the family is there from the first frame.
pub fn preload() {
    if let Some(bytes) = cache_file().and_then(|path| std::fs::read(path).ok()) {
        if set(&bytes) {
            LIVE.store(true, SeqCst);
        }
    }
}

/// Starts the download on a thread when the cache had nothing. Failure is logged, nothing more.
pub fn fetch_if_missing(ctx: &egui::Context) {
    if FONT.get().is_some() || FETCHING.swap(true, SeqCst) {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .user_agent(zenith_trade::client::user_agent())
                .timeout_global(Some(Duration::from_secs(20)))
                .build()
                .into();
            let mut response = agent.get(URL).call().map_err(|e| e.to_string())?;
            let bytes = response.body_mut().read_to_vec().map_err(|e| e.to_string())?;
            if woff_to_sfnt(&bytes).is_none() {
                return Err("not a WOFF file".into());
            }
            if let Some(path) = cache_file() {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(path, &bytes);
            }
            set(&bytes);
            Ok(())
        })();
        match result {
            Ok(()) => ctx.request_repaint(),
            Err(error) => eprintln!("fontin: download failed: {error}"),
        }
    });
}

/// The decoded font, once there.
pub fn sfnt() -> Option<&'static [u8]> {
    FONT.get().map(|font| font.sfnt.as_slice())
}

/// The family of the font, for text that may use it (see `live`).
pub fn family() -> egui::FontFamily {
    egui::FontFamily::Name("fontin".into())
}

/// Ascender and descender per em.
pub fn metrics_em() -> (f32, f32) {
    FONT.get().map_or((0.9, 0.25), |font| font.metrics)
}

/// True once egui has the family: draw with it only then. A font that arrived late is installed
/// here (`install`), and is live at the frame after.
pub fn live(ctx: &egui::Context, install: impl FnOnce(&egui::Context)) -> bool {
    if LIVE.load(SeqCst) {
        return true;
    }
    if FONT.get().is_none() {
        return false;
    }
    let now = ctx.cumulative_pass_nr() + 1;
    match PENDING.load(SeqCst) {
        0 => {}
        at if now > at => {
            LIVE.store(true, SeqCst);
            return true;
        }
        _ => return false,
    }
    PENDING.store(now, SeqCst);
    install(ctx);
    ctx.request_repaint();
    false
}
