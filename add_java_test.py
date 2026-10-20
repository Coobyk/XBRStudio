#!/usr/bin/env python3
"""Add a test for the variable-parent style and check the tree."""
import sys

p = "src/java.rs"
s = open(p).read()

old = """    #[test]
    fn parses_java_numbers() {"""

new = '''    const COW_VARIABLE: &str = r#"
package net.minecraft.client.model.animal.cow;

public class CowModel {
    public static LayerDefinition createBodyLayer() {
        MeshDefinition meshdefinition = new MeshDefinition();
        PartDefinition partdefinition = meshdefinition.getRoot()
            .addOrReplaceChild("head", CubeListBuilder.create().texOffs(0, 0)
                .addBox(-4.0F, -4.0F, -6.0F, 8.0F, 8.0F, 6.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        partdefinition.addOrReplaceChild("nose", CubeListBuilder.create().texOffs(22, 0)
            .addBox(-1.0F, -1.0F, -1.0F, 2.0F, 3.0F, 1.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        PartDefinition partdefinition1 = meshdefinition.getRoot()
            .addOrReplaceChild("body", CubeListBuilder.create().texOffs(16, 16)
                .addBox(-4.0F, 0.0F, -2.0F, 8.0F, 12.0F, 4.0F, new CubeDeformation(0.0F, 0.0F, 0.0F)), PartPose.ZERO);
        return LayerDefinition.create(meshdefinition, 64, 64);
    }
}
"#;

    #[test]
    fn finds_children_that_are_added_through_a_variable() {
        let models = extract_file(COW_VARIABLE, "CowModel.java");
        assert_eq!(models.len(), 1);
        let parts = &models[0].parts;
        assert_eq!(parts.len(), 2, "head and body are roots");
        assert_eq!(parts[0].name, "head");
        assert_eq!(parts[1].name, "body");
        assert_eq!(parts[0].children.len(), 1, "nose belongs to the head");
        assert_eq!(parts[0].children[0].name, "nose");
        assert_eq!(parts[0].children[0].cubes[0].uv, [22.0, 0.0]);
    }

    #[test]
    fn parses_java_numbers() {'''

assert old in s
s = s.replace(old, new, 1)
open(p, "w").write(s)
print("variable parent test added")
