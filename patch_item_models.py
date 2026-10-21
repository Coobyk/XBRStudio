#!/usr/bin/env python3
"""Stitch the item models as well and report the assets that were read."""
import sys


def patch(path, pairs):
    with open(path, "r", encoding="utf-8") as handle:
        source = handle.read()
    for old, new in pairs:
        if old not in source:
            print("MISSING in %s:\n%s" % (path, old[:300]))
            sys.exit(1)
        if source.count(old) != 1:
            print("NOT UNIQUE in %s (%d):\n%s" % (path, source.count(old), old[:300]))
            sys.exit(1)
        source = source.replace(old, new)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    print("patched", path)


patch(
    "src/assets.rs",
    [
        (
            """        assert_eq!(sprites, vec!["block/stone", "item/diamond_sword"]);""",
            """        assert_eq!(
            sprites,
            vec!["block/stone.png", "item/diamond_sword.png"]
        );""",
        ),
        (
            """    #[test]
    fn reads_an_item_model() {
        let entries = vec![(
            "assets/minecraft/models/item/diamond_sword.json".to_string(),
            serde_json::to_vec(&json!({
                "parent": "minecraft:item/generated",
                "textures": {"layer0": "minecraft:item/diamond_sword"}
            }))
            .unwrap(),
        )];
        let models = load_item_models(&entries);
        assert!(models.contains_key("diamond_sword"));
    }""",
            """    #[test]
    fn reads_an_item_model() {
        // The item models are mostly flat, only a few have geometry.
        let entries = vec![(
            "assets/minecraft/models/item/spyglass_in_hand.json".to_string(),
            serde_json::to_vec(&json!({
                "textures": {"spyglass": "minecraft:item/spyglass_model"},
                "elements": [{
                    "from": [7, 8.5, 7],
                    "to": [9, 13.5, 9],
                    "faces": {"north": {"uv": [0, 2, 2, 7]}}
                }]
            }))
            .unwrap(),
        )];
        let models = load_item_models(&entries);
        assert!(models.contains_key("spyglass_model"), "{models:?}");
    }""",
        ),
    ],
)

# The batch reads the item models as well.
patch(
    "src/jar.rs",
    [
        (
            """    /// The block models of the source, by their short name.
    block_model_entries: Vec<(String, Vec<u8>)>,
    pack_format: Option<i64>,
}""",
            """    /// The block models of the source, by their short name.
    block_model_entries: Vec<(String, Vec<u8>)>,
    /// The item models of the source.
    item_model_entries: Vec<(String, Vec<u8>)>,
    /// The sprites of the atlases of the source.
    atlas_entries: Vec<(String, Vec<u8>)>,
    pack_format: Option<i64>,
}""",
        ),
        (
            """    let block_models = if opts.stitch {
        load_block_models(&source.block_model_entries)
    } else {
        HashMap::new()
    };""",
            """    let block_models = if opts.stitch {
        let mut models = load_block_models(&source.block_model_entries);
        // The item models have the same layout, so they can be stitched too.
        for (stem, faces) in crate::assets::load_item_models(&source.item_model_entries) {
            models.entry(stem).or_insert(faces);
        }
        models
    } else {
        HashMap::new()
    };""",
        ),
    ],
)
