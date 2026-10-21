#!/usr/bin/env python3
"""Replace the atlas test with the current format."""
import sys

p = "src/assets.rs"
s = open(p).read()

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
        assert_eq!(sprites, vec!["block/stone.png", "item/diamond_sword.png"]);
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
print("atlas tests updated")
