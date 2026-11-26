//! Currency icons, fetched at run time from the game's content network and kept in
//! `%APPDATA%\Zenith Trade\icons\<id>.png`. None is part of the repository. A key is an exchange
//! id (`chaos`) or an item name (`Divine Orb`). The download goes on its own thread and the panel
//! asks again every frame: until the picture is there, the text stands in its place.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use zenith_trade::exchange::{image_url, images};

enum Slot {
    Loading,
    Ready(Arc<ColorImage>),
    Missing,
}

static SLOTS: Mutex<Option<HashMap<String, Slot>>> = Mutex::new(None);
/// The static data's images, fetched once.
type Index = HashMap<String, (String, String)>;
static INDEX: OnceLock<Mutex<Option<Index>>> = OnceLock::new();

/// The exchange id a name of the panel stands for (`div` is `divine`).
pub fn id_of(unit: &str) -> &str {
    match unit {
        "div" => "divine",
        "c" => "chaos",
        other => other,
    }
}

pub fn cache_dir() -> Option<PathBuf> {
    Some(crate::config::data_dir().join("icons"))
}

/// The file of a key's icon: its name lowercased, any character that is no letter, digit, `-` or
/// `_` turned into `_`. `None` for a key with nothing in it.
pub fn cache_path(dir: &Path, key: &str) -> Option<PathBuf> {
    let name: String = key
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if name.is_empty() || name.chars().all(|c| c == '_') {
        return None;
    }
    Some(dir.join(format!("{name}.png")))
}

/// The icon of `key` as a texture of `ctx`, once it is there. The first call starts the
/// download if the cache has no file; `None` until then, and for good when it failed.
pub fn get(ctx: &egui::Context, key: &str) -> Option<TextureId> {
    get_with(ctx, key, None)
}

/// The same for an item that is not in the static data (a unique): `url` is the picture's own
/// address, as the trade site's listings give it, and `key` the name it is kept under.
pub fn get_url(ctx: &egui::Context, key: &str, url: &str) -> Option<TextureId> {
    get_with(ctx, key, Some(url))
}

/// Where poe.ninja lists the pictures of the uniques, by overview type.
const UNIQUE_SOURCE: &str = "https://poe.ninja/poe1/api/economy/stash/current/item/overview";
/// A cached map of pictures is fetched again after this long.
const UNIQUE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// The poe.ninja overview that lists the uniques of an item class.
pub fn unique_type(class: &str) -> Option<&'static str> {
    Some(match class {
        "Rings" | "Amulets" | "Belts" => "UniqueAccessory",
        "Helmets" | "Body Armours" | "Gloves" | "Boots" | "Shields" | "Quivers" => "UniqueArmour",
        "Jewels" | "Abyss Jewels" => "UniqueJewel",
        "Life Flasks" | "Mana Flasks" | "Hybrid Flasks" | "Utility Flasks" => "UniqueFlask",
        "Maps" => "UniqueMap",
        "Bows" | "Claws" | "Daggers" | "Rune Daggers" | "Wands" | "Sceptres" | "Staves" | "Warstaves" | "One Hand Swords" | "Thrusting One Hand Swords" | "Two Hand Swords"
        | "One Hand Axes" | "Two Hand Axes" | "One Hand Maces" | "Two Hand Maces" | "Fishing Rods" => "UniqueWeapon",
        _ => return None,
    })
}

/// The picture address of each name in a per-type overview (`{"lines": [{"name", "icon"}]}`): the
/// first one a name has.
pub fn parse_unique_icons(body: &str) -> std::collections::HashMap<String, String> {
    let mut icons = std::collections::HashMap::new();
    let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    for line in value["lines"].as_array().into_iter().flatten() {
        if let (Some(name), Some(icon)) = (line["name"].as_str(), line["icon"].as_str())
            && !name.is_empty()
            && !icon.is_empty()
        {
            icons.entry(name.to_owned()).or_insert_with(|| icon.to_owned());
        }
    }
    icons
}

/// The picture address of each currency in a currency overview
/// (`{"currencyDetails": [{"name", "icon"}]}`): the first one a name has.
pub fn parse_currency_icons(body: &str) -> std::collections::HashMap<String, String> {
    let mut icons = std::collections::HashMap::new();
    let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    for line in value["currencyDetails"].as_array().into_iter().flatten() {
        if let (Some(name), Some(icon)) = (line["name"].as_str(), line["icon"].as_str())
            && !name.is_empty()
            && !icon.is_empty()
        {
            icons.entry(name.to_owned()).or_insert_with(|| icon.to_owned());
        }
    }
    icons
}

/// Where poe.ninja lists the pictures of the currencies and fragments, by overview type.
const CURRENCY_SOURCE: &str = "https://poe.ninja/poe1/api/economy/stash/current/currency/overview";

/// The kinds of the currency endpoint; every other kind is an item overview.
fn is_currency_kind(kind: &str) -> bool {
    matches!(kind, "Currency" | "Fragment")
}

/// The pictures of one overview type (uniques, skill gems, currencies, fragments), read from the
/// cache file in `dir` when it is under a day old, else fetched (and the file written): one
/// request per kind and league. Empty when it can be neither.
pub fn unique_icons(dir: &Path, league: &str, kind: &str, user_agent: &str) -> std::collections::HashMap<String, String> {
    let safe: String = league.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let file = dir.join(format!("unique-icons-{kind}-{safe}.json"));
    let fresh = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok().and_then(|at| at.elapsed().ok()).is_some_and(|age| age < UNIQUE_MAX_AGE);
    if fresh && let Some(map) = std::fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str(&text).ok()) {
        return map;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(user_agent)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(8)))
        .build()
        .into();
    let currency = is_currency_kind(kind);
    let source = if currency { CURRENCY_SOURCE } else { UNIQUE_SOURCE };
    let body = agent.get(source).query("league", league).query("type", kind).call().ok().and_then(|mut response| response.body_mut().with_config().limit(16 * 1024 * 1024).read_to_string().ok());
    let map = body.map(|body| if currency { parse_currency_icons(&body) } else { parse_unique_icons(&body) }).unwrap_or_default();
    if !map.is_empty() {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(text) = serde_json::to_string(&map) {
            let _ = std::fs::write(&file, text);
        }
    }
    map
}

/// The wiki file of an influence's item symbol (or of the Breach logo, for `Breach`), by the label
/// the panel gives it.
pub fn symbol_file(label: &str) -> Option<&'static str> {
    Some(match label {
        "Shaper" => "Shaper-item-symbol.png",
        "Elder" => "Elder-item-symbol.png",
        "Crusader" => "Crusader-item-symbol.png",
        "Redeemer" => "Redeemer-item-symbol.png",
        "Hunter" => "Hunter-item-symbol.png",
        "Warlord" => "Warlord-item-symbol.png",
        "Searing Exarch" => "Searing-exarch-item-symbol.png",
        "Eater of Worlds" => "Eater-of-worlds-item-symbol.png",
        "Synthesised" => "Synthesised-item-symbol.png",
        "Breach" => "Breach_logo.png",
        "Veiled" => "Veiled_Chaos_Orb_inventory_icon.png",
        "Unveiled" => "Veiled_Chaos_Orb_inventory_icon.png",
        "Essence" => "Deafening_Essence_of_Greed_inventory_icon.png",
        "Delve" => "Pristine_Fossil_inventory_icon.png",
        _ => return None,
    })
}

/// The symbol of an influence (or `Breach`) as a texture, once it is there. It comes from the
/// community wiki, once, and is kept in the cache like the other icons.
pub fn symbol(ctx: &egui::Context, label: &str) -> Option<TextureId> {
    let file = symbol_file(label)?;
    get_with(ctx, &format!("wiki-{}", label.to_lowercase().replace(' ', "-")), Some(&format!("wiki:{file}")))
}

/// The address that tells where a wiki file is.
fn wiki_api_url(file: &str) -> String {
    format!("https://www.poewiki.net/w/api.php?action=query&titles=File:{file}&prop=imageinfo&iiprop=url&format=json")
}

/// The file's address in the wiki's answer.
fn wiki_image_url(answer: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(answer).ok()?;
    let pages = json.pointer("/query/pages")?.as_object()?;
    pages.values().find_map(|page| page.pointer("/imageinfo/0/url")?.as_str().map(str::to_owned)).filter(|url| url.starts_with("https://"))
}

fn get_with(ctx: &egui::Context, key: &str, url: Option<&str>) -> Option<TextureId> {
    if cfg!(test) {
        return None;
    }
    let key = id_of(key);
    let id = egui::Id::unique(("zenith-icon", key));
    if let Some(handle) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return Some(handle.id());
    }
    let mut guard = SLOTS.lock().ok()?;
    let slots = guard.get_or_insert_with(HashMap::new);
    if !slots.contains_key(key) {
        // A file already in the cache is a few kilobytes: read here. Only the network waits.
        let cached = cache_dir()
            .and_then(|dir| cache_path(&dir, key))
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| decode(&bytes).ok())
            .map(|image| fit(key, image));
        if let Some(image) = cached {
            slots.insert(key.to_owned(), Slot::Ready(Arc::new(image)));
        } else {
            slots.insert(key.to_owned(), Slot::Loading);
            let (ctx, key, url) = (ctx.clone(), key.to_owned(), url.map(str::to_owned));
            let started = std::thread::Builder::new().name("zenith-icon".into()).spawn(move || {
                let slot = match download_from(&key, url.as_deref()) {
                    Ok(image) => Slot::Ready(Arc::new(fit(&key, image))),
                    Err(error) => {
                        log!("icon {key}: {error}");
                        Slot::Missing
                    }
                };
                if let Ok(mut slots) = SLOTS.lock() {
                    slots.get_or_insert_with(HashMap::new).insert(key, slot);
                }
                ctx.request_repaint();
            });
            if started.is_err() {
                log!("icon thread did not start");
            }
        }
    }
    match slots.get(key) {
        Some(Slot::Ready(image)) => {
            let handle = ctx.load_texture(format!("icon-{key}"), image.as_ref().clone(), TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)));
            let texture = handle.id();
            ctx.data_mut(|data| data.insert_temp(id, handle));
            Some(texture)
        }
        Some(Slot::Loading | Slot::Missing) | None => None,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(zenith_trade::client::user_agent())
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

fn fetch(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut response = agent.get(url).call().map_err(|error| error.to_string())?;
    response.body_mut().read_to_vec().map_err(|error| error.to_string())
}

/// Downloads the icon of `key` (unless the cache has it), writes it to the cache and decodes it.
fn download_from(key: &str, url: Option<&str>) -> Result<ColorImage, String> {
    let dir = cache_dir().ok_or("no cache folder")?;
    let path = cache_path(&dir, key).ok_or("no key")?;
    if let Some(image) = std::fs::read(&path).ok().and_then(|bytes| decode(&bytes).ok()) {
        return Ok(image);
    }
    let agent = agent();
    let resolved = match url.and_then(|url| url.strip_prefix("wiki:")) {
        Some(file) => {
            let answer = String::from_utf8(fetch(&agent, &wiki_api_url(file))?).map_err(|_| "wiki answer is not text")?;
            Some(wiki_image_url(&answer).ok_or("not on the wiki")?)
        }
        None => None,
    };
    if let Some(url) = resolved.as_deref().or(url).filter(|url| url.starts_with("https://")) {
        let bytes = fetch(&agent, url)?;
        let decoded = decode(&bytes)?;
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let _ = std::fs::write(&path, &bytes);
        return Ok(decoded);
    }
    let index = INDEX.get_or_init(|| Mutex::new(None));
    let (id, image) = {
        let mut index = index.lock().map_err(|_| "poisoned")?;
        if index.is_none() {
            let url = format!("{}/api/trade/data/static", zenith_trade::client::BASE_URL);
            let text = String::from_utf8(fetch(&agent, &url)?).map_err(|_| "static data is not text")?;
            let json = serde_json::from_str(&text).map_err(|_| "static data is not JSON")?;
            *index = Some(images(&json));
        }
        index.as_ref().and_then(|index| index.get(&key.to_lowercase())).cloned().ok_or("not in the static data")?
    };
    let bytes = fetch(&agent, &image_url(&image))?;
    let decoded = decode(&bytes)?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    // Written under the key and under the exchange id, so the next run finds either.
    for name in [path.clone(), cache_path(&dir, &id).unwrap_or(path)] {
        let _ = std::fs::write(name, &bytes);
    }
    Ok(decoded)
}

/// The part of an image that makes a square symbol. The Breach logo is a wide banner: only its
/// hand and violet flame, centred on a square, read at pill size. Any other image that is not
/// square, as a belt or a two-handed weapon, is centred on a clear square, so that it keeps its
/// shape where icons are drawn square.
fn fit(key: &str, image: ColorImage) -> ColorImage {
    let [width, height] = image.size;
    if key != "wiki-breach" || width < 420 || height < 275 {
        return squared(image);
    }
    // The hand and its flame, in the 662 × 375 logo, scaled if the file is another size.
    let scale = width as f32 / 662.0;
    let (x0, w, h) = ((250.0 * scale) as usize, (170.0 * scale) as usize, ((275.0 * scale) as usize).min(height));
    let side = h.max(w);
    let left = (side - w) / 2;
    let mut out = ColorImage::new([side, side], vec![egui::Color32::TRANSPARENT; side * side]);
    for y in 0..h {
        for x in 0..w {
            out.pixels[y * side + left + x] = image.pixels[y * width + x0 + x];
        }
    }
    out
}

/// `image` centred on a clear square as wide as its longer side.
fn squared(image: ColorImage) -> ColorImage {
    let [width, height] = image.size;
    if width == height {
        return image;
    }
    let side = width.max(height);
    let (left, top) = ((side - width) / 2, (side - height) / 2);
    let mut out = ColorImage::new([side, side], vec![egui::Color32::TRANSPARENT; side * side]);
    for y in 0..height {
        let row = (top + y) * side + left;
        out.pixels[row..row + width].copy_from_slice(&image.pixels[y * width..(y + 1) * width]);
    }
    out
}

/// A PNG as an egui image.
pub fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("too large")?];
    let info = reader.next_frame(&mut buffer).map_err(|error| error.to_string())?;
    let data = &buffer[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed colour was not expanded".into()),
    };
    Ok(ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &rgba))
}
