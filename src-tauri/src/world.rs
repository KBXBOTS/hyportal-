//! World settings — what the settings panel edits, and how they reach the server.
//!
//! The Hytale server keeps its own JSON config: `config.json` in the universe
//! root for server-wide settings, and `worlds/<name>/config.json` for the world
//! itself. It owns both files, rewrites them on shutdown, and fills them with
//! keys we know nothing about. So HyPortal stores its own copy of the handful of
//! settings it exposes and merges them in just before launch, touching only
//! those keys and leaving everything else exactly as the server left it.
//!
//! Every value written here comes from the server's own config schema
//! (`--generate-config-schema`) or from the world presets Hytale ships in
//! `Server/Instances/Defaults/`. Nothing is guessed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// HyPortal hosts a single world. The server still wants it under a name.
pub const WORLD: &str = "default";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub server_name: String,
    pub motd: String,
    /// Blank means anyone with the address can join.
    pub password: String,
    pub max_players: u32,
    pub view_radius: u32,
    pub port: u16,

    /// One of the keys in [`world_gen`].
    pub world_type: String,
    /// Blank means "whatever the world already has"; text is hashed, see [`seed_value`].
    pub seed: String,
    /// `Adventure` or `Creative` — the only two the server accepts.
    pub game_mode: String,

    pub cheats: bool,
    pub pvp: bool,
    pub fall_damage: bool,
    pub freeze_time: bool,
    pub spawn_npcs: bool,
    pub keep_inventory: bool,
    pub whitelist: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_name: "My Hytale World".into(),
            motd: String::new(),
            password: String::new(),
            max_players: 10,
            view_radius: 32,
            port: 5520,

            world_type: "normal".into(),
            seed: String::new(),
            game_mode: "Adventure".into(),

            // On by default: a world you host for friends is one you can fix.
            cheats: true,
            pvp: false,
            fall_damage: true,
            freeze_time: false,
            spawn_npcs: true,
            keep_inventory: false,
            whitelist: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Settings,
    /// True once a world has been generated on disk. From then on, changing the
    /// world type or seed does nothing until the world is reset — the chunks are
    /// already written and the generator never revisits them.
    pub world_exists: bool,
    pub problem: Option<String>,
}

/// Where HyPortal keeps its worlds — deliberately separate from the folder the
/// official launcher uses, so nothing here can touch a world you already care
/// about.
pub fn universe() -> Option<PathBuf> {
    crate::hytale::detect()
        .user_data
        .map(|d| PathBuf::from(d).join("HyPortalWorlds"))
}

/// HyPortal's own settings file, kept outside the universe so the server's
/// rewriting of `config.json` can never clobber it.
fn store_path() -> Option<PathBuf> {
    crate::hytale::detect()
        .user_data
        .map(|d| PathBuf::from(d).join("HyPortalSettings.json"))
}

pub fn load() -> Settings {
    store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Clamp anything a text field could make nonsensical, then persist.
///
/// Returns what was actually stored so the panel can show the corrected values
/// rather than silently disagreeing with disk.
pub fn save(mut s: Settings) -> Result<Settings, String> {
    s.server_name = s.server_name.trim().to_string();
    if s.server_name.is_empty() {
        s.server_name = "My Hytale World".into();
    }
    s.motd = s.motd.trim().to_string();
    s.password = s.password.trim().to_string();
    s.seed = s.seed.trim().to_string();
    s.max_players = s.max_players.clamp(1, 500);
    s.view_radius = s.view_radius.clamp(4, 64);
    if s.port == 0 {
        s.port = 5520;
    }
    if s.game_mode != "Creative" {
        s.game_mode = "Adventure".into();
    }
    if world_gen(&s.world_type).is_none() {
        s.world_type = "normal".into();
    }

    let path = store_path().ok_or("No Hytale user folder to save settings in.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&s).map_err(|e| format!("Could not encode settings: {e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("Could not save settings: {e}"))?;
    Ok(s)
}

pub fn view() -> SettingsView {
    let universe = universe();
    SettingsView {
        settings: load(),
        world_exists: universe
            .as_ref()
            .map(|u| u.join("worlds").join(WORLD).exists())
            .unwrap_or(false),
        problem: universe
            .is_none()
            .then(|| "No Hytale installation found, so there is nowhere to keep a world.".into()),
    }
}

/// The generator config for each world type.
///
/// These are lifted verbatim from the presets Hytale ships in
/// `Server/Instances/Defaults/*/instance.bson` and
/// `Server/HytaleGenerator/WorldStructures/`, so they are the same generators
/// the game itself uses rather than something reconstructed.
fn world_gen(kind: &str) -> Option<Value> {
    let structure = |name: &str| json!({ "Type": "HytaleGenerator", "WorldStructure": name });
    Some(match kind {
        "normal" => json!({ "Type": "Hytale", "Name": "Default" }),
        "flat" => structure("Default_Flat"),
        "void" => structure("Default_Void"),
        "plains" => structure("Zone1_Plains1"),
        "desert" => structure("Zone2_Desert1"),
        "taiga" => structure("Zone3_Taiga1"),
        "volcanic" => structure("Zone4_Volcanic1"),
        _ => return None,
    })
}

/// Flat and void worlds have no terrain for the generator to place a spawn on,
/// so Hytale's own presets pin one at y 80. Same point, same reason.
fn fixed_spawn(kind: &str) -> Option<Value> {
    matches!(kind, "flat" | "void").then(|| {
        json!({
            "Id": "Global",
            "SpawnPoint": { "X": 0.5, "Y": 80, "Z": 0.5, "Pitch": 0, "Yaw": 180, "Roll": 0.0 }
        })
    })
}

/// Turn whatever was typed in the seed box into the integer the server wants.
///
/// A plain number is used as-is. Anything else becomes its Java string hash,
/// the same convention Minecraft uses, so typing the same word always gives you
/// the same world.
fn seed_value(seed: &str) -> Option<i64> {
    let s = seed.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let mut h: i32 = 0;
    for c in s.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as i32);
    }
    Some(h as i64)
}

fn read_object(path: &Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

fn write_object(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(map.clone()))
        .map_err(|e| format!("Could not encode {}: {e}", path.display()))?;
    std::fs::write(path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// Merge the settings into the server's own config files.
///
/// Read-modify-write throughout: only the keys the settings panel controls are
/// touched, so a config the server has enriched keeps everything else intact.
pub fn apply(universe: &Path, s: &Settings) -> Result<(), String> {
    // ---- server-wide ----
    let server_path = universe.join("config.json");
    let mut server = read_object(&server_path);
    server.insert("ServerName".into(), json!(s.server_name));
    server.insert("MOTD".into(), json!(s.motd));
    server.insert("Password".into(), json!(s.password));
    server.insert("MaxPlayers".into(), json!(s.max_players));
    server.insert("MaxViewRadius".into(), json!(s.view_radius));
    server.insert(
        "Defaults".into(),
        json!({ "World": WORLD, "GameMode": s.game_mode }),
    );
    write_object(&server_path, &server)?;

    // ---- the world itself ----
    let world_path = universe.join("worlds").join(WORLD).join("config.json");
    let mut world = read_object(&world_path);

    if let Some(gen) = world_gen(&s.world_type) {
        world.insert("WorldGen".into(), gen);
    }
    match fixed_spawn(&s.world_type) {
        Some(spawn) => world.insert("SpawnProvider".into(), spawn),
        // Terrain worlds place their own spawn; leaving a stale pin behind would
        // drop players into the middle of a hillside.
        None => world.remove("SpawnProvider"),
    };
    if let Some(seed) = seed_value(&s.seed) {
        world.insert("Seed".into(), json!(seed));
    }

    world.insert("GameMode".into(), json!(s.game_mode));
    world.insert("IsPvpEnabled".into(), json!(s.pvp));
    world.insert("IsFallDamageEnabled".into(), json!(s.fall_damage));
    world.insert("IsGameTimePaused".into(), json!(s.freeze_time));
    world.insert("IsSpawningNPC".into(), json!(s.spawn_npcs));

    // An inline Death block overrides the gameplay config, which by default
    // drops half your items. Only write one when we actually want to override —
    // removing it hands the decision back to the game's own balance.
    if s.keep_inventory {
        world.insert("Death".into(), json!({ "ItemsLossMode": "None" }));
    } else {
        world.remove("Death");
    }

    write_object(&world_path, &world)?;

    // ---- whitelist ----
    let list_path = universe.join("whitelist.json");
    let mut list = read_object(&list_path);
    list.insert("enabled".into(), json!(s.whitelist));
    list.entry("list").or_insert_with(|| json!([]));
    write_object(&list_path, &list)?;

    Ok(())
}

/// Delete the generated world so the next start builds a fresh one.
///
/// The only way a changed world type or seed can take effect: the chunks are
/// already on disk and the generator never revisits them.
pub fn reset_world() -> Result<(), String> {
    let universe = universe().ok_or("No Hytale installation found.")?;
    let dir = universe.join("worlds").join(WORLD);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete the world: {e}"))?;
    }
    Ok(())
}
