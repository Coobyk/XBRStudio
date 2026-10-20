#!/usr/bin/env python3
"""Add a test for several texOffs in one chain."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """    #[test]
    fn keeps_unresolvable_expressions_as_text() {"""

new = '''    #[test]
    fn every_texoffs_applies_to_the_boxes_that_follow_it() {
        let cubes = cubes_of(
            "CubeListBuilder.create().texOffs(0, 0)\\
             .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))\\
             .texOffs(1, 33)\\
             .addBox(-3.0F, 1.0F, -7.0F, 6.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F))",
        );
        assert_eq!(cubes.len(), 2);
        assert_eq!(cubes[0].uv, [0.0, 0.0]);
        assert_eq!(cubes[1].uv, [1.0, 33.0]);
        assert_eq!(cubes[1].origin, [-3.0, 1.0, -7.0]);
        assert_eq!(cubes[1].size, [6.0, 3.0, 1.0]);
    }

    #[test]
    fn keeps_unresolvable_expressions_as_text() {'''

assert old in s
s = s.replace(old, new, 1)
open(p, "w").write(s)
print("texOffs test added")
