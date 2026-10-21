#!/usr/bin/env python3
"""Read the atlases the way the game does."""
import sys

p = "src/assets.rs"
s = open(p).read()

start = s.index("/// The sprites of the atlases of a jar: the texture paths they hold.")
end = s.index("/// The item models of a jar, by the texture they use.")

new = '''/// The atlases of the game.
///
/// An atlas holds sprites: either a whole directory of textures or a single
/// texture. The format changed over the years, so both the old (`source`)
/// and the new (`resource` and `prefix`) forms are read.
#[derive(Debug, Clone, Default)]
pub struct Atlases {
    /// The directories whose textures are packed, e.g. `block/`.
    pub directories: Vec<String>,
    /// The single sprites of the atlases, e.g. `entity/bell/bell_body.png`.
    pub sprites: Vec<String>,
}

impl Atlases {
    /// True when the texture (relative to `textures/`) is a sprite.
    pub fn contains(&self, relative: &str) -> bool {
        if self.directories.is_empty() && self.sprites.is_empty() {
            return true;
        }
        if self.sprites.iter().any(|sprite| sprite == relative) {
            return true;
        }
        self.directories
            .iter()
            .any(|directory| relative.starts_with(directory.as_str()))
    }
}

/// Read the atlases of a jar or a pack folder.
pub fn parse_atlases(entries: &[(String, Vec<u8>)]) -> Atlases {
    let mut atlases = Atlases::default();
    for (name, bytes) in entries {
        if !name.starts_with("assets/minecraft/atlases/") || !name.ends_with(".json") {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            continue;
        };
        let Some(sources) = value.get("sources").and_then(Value::as_array) else {
            continue;
        };
        for source in sources {
            if let Some(prefix) = source.get("prefix").and_then(Value::as_str) {
                // A whole directory of textures.
                let prefix = prefix.trim_start_matches('/');
                atlases
                    .directories
                    .push(prefix.trim_end_matches('/').to_string() + "/");
                continue;
            }
            if let Some(resource) = source.get("resource").and_then(Value::as_str) {
                // A single texture.
                atlases
                    .sprites
                    .push(sprite_path(resource));
                continue;
            }
            if let Some(source) = source.get("source").and_then(Value::as_str) {
                if source.starts_with('#') {
                    continue;
                }
                let path = source
                    .rsplit(':')
                    .next()
                    .unwrap_or(source)
                    .trim_start_matches('/');
                if !path.contains('/') {
                    // The old format: a directory of textures.
                    atlases.directories.push(format!("{path}/"));
                    continue;
                }
                atlases.sprites.push(sprite_path(path));
            }
        }
    }
    atlases.directories.sort();
    atlases.directories.dedup();
    atlases.sprites.sort();
    atlases.sprites.dedup();
    atlases
}

/// The path of a sprite, relative to `textures/`.
fn sprite_path(path: &str) -> String {
    let path = path
        .rsplit(':')
        .next()
        .unwrap_or(path)
        .trim_start_matches('/');
    let path = path.strip_prefix("textures/").unwrap_or(path);
    let path = path.strip_suffix(".png").unwrap_or(path);
    format!("{path}.png")
}

'''

s = s[:start] + new + s[end:]

# The test.
old = """    #[test]
    fn reads_the_sprites_of_an_atlas() {
        let entries = vec![(
            "assets/minecraft/atlases/blocks.json".to_string(),
            serde_json::to_vec(&json!({
                "sources": [
                    {"source": "minecraft:block/stone"},
                    {"source": "#minecraft:placeholder"},
                    {"source": "minecraft:item/diamond_sword"}
                ]
            }))
            .unwrap(),
        )];
        let sprites = parse_atlases(&entries);
        assert_eq!(
            sprites,
            vec!["block/stone.png", "item/diamond_sword.png"]
        );
    }"""
new = """    #[test]
    fn reads_the_sprites_of_an_atlas() {
        // The format of the game: a directory of textures and a single one.
        let entries = vec![(
            "assets/minecraft/atlases/blocks.json".to_string(),
            serde_json::to_vec(&json!({
                "sources": [
                    {"type": "minecraft:directory", "prefix": "block/", "source": "block"},
                    {"type": "minecraft:single", "resource": "minecraft:entity/bell/bell_body"}
                ]
            }))
            .unwrap(),
        )];
        let atlases = parse_atlases(&entries);
        assert_eq!(atlases.directories, vec!["block/"]);
        assert_eq!(atlases.sprites, vec!["entity/bell/bell_body.png"]);
        assert!(atlases.contains("block/stone.png"));
        assert!(atlases.contains("entity/bell/bell_body.png"));
        assert!(!atlases.contains("item/diamond_sword.png"));
    }

    #[test]
    fn reads_the_old_atlas_format() {
        let entries = vec![(
            "assets/minecraft/atlases/items.json".to_string(),
            serde_json::to_vec(&json!({
                "sources": [
                    {"source": "minecraft:item/diamond_sword"},
                    {"source": "#minecraft:placeholder"}
                ]
            }))
            .unwrap(),
        )];
        let atlases = parse_atlases(&entries);
        assert_eq!(atlases.sprites, vec!["item/diamond_sword.png"]);
        assert!(atlases.contains("item/diamond_sword.png"));
    }"""
assert old in s
s = s.replace(old, new)

open(p, "w").write(s)
print("assets.rs reads the atlases")
