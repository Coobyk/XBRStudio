#!/usr/bin/env python3
"""Rewrite cubes_of so texOffs and addBox are read in order."""
import sys

p = "src/java.rs"
s = open(p).read()

start = s.index("/// The cubes of a `CubeListBuilder` chain.")
end = s.index("/// The translation and the rotation of a `PartPose`.")

new_block = r'''/// The cubes of a `CubeListBuilder` chain.
///
/// The chain is read in order, because every `texOffs` only applies to the
/// boxes that follow it:
///
/// ```java
/// CubeListBuilder.create().texOffs(0, 0)
///         .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))
///         .texOffs(1, 33)
///         .addBox(-3.0F, 1.0F, -7.0F, 6.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))
/// ```
fn cubes_of(chain: &str) -> Vec<ExtractedCube> {
    let mut cubes = Vec::new();
    let mut uv = [0.0f32, 0.0f32];
    let mut search = 0usize;

    while search < chain.len() {
        let next_tex = chain[search..].find("texOffs(");
        let next_box = chain[search..].find("addBox(");

        // Whichever comes first.
        let take_tex = match (next_tex, next_box) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(tex), Some(box_)) => tex <= box_,
        };

        if take_tex {
            let open = search + next_tex.unwrap() + "texOffs".len();
            let Some(close) = matching_delimiter(chain, open, b'(', b')') else {
                break;
            };
            let args = split_args(&chain[open + 1..close]);
            let numbers: Vec<f32> = args.iter().filter_map(|arg| parse_number(arg)).collect();
            if numbers.len() >= 2 {
                uv = [numbers[0], numbers[1]];
            }
            search = close + 1;
            continue;
        }

        let open = search + next_box.unwrap() + "addBox".len();
        let Some(close) = matching_delimiter(chain, open, b'(', b')') else {
            break;
        };
        let args = split_args(&chain[open + 1..close]);
        let numbers: Vec<f32> = args.iter().filter_map(|arg| parse_number(arg)).collect();
        if numbers.len() >= 6 {
            let origin = [numbers[0], numbers[1], numbers[2]];
            let size = [numbers[3], numbers[4], numbers[5]];
            let deform = args.get(6).map(|arg| arg.trim().to_string()).filter(|text| {
                !text.is_empty()
                    && !text
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == ' ')
            });
            let mirror = args.get(7).map(|arg| arg.trim() == "true").unwrap_or(false);
            cubes.push(ExtractedCube {
                origin,
                size,
                uv,
                deform,
                mirror,
            });
        }
        search = close + 1;
    }

    cubes
}

'''

s = s[:start] + new_block + s[end:]
open(p, "w").write(s)
print("cubes_of rewritten")
