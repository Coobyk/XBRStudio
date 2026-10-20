#!/usr/bin/env python3
"""Give every pack test its own directory."""
import sys

p = "src/pack.rs"
s = open(p).read()

s = s.replace(
    """    #[test]
    fn writes_pack_mcmeta_with_the_pack_format() {
        let dir = std::env::temp_dir().join(format!("xbrstudio-pack-{}", std::process::id()));""",
    """    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("xbrstudio-{name}-{}", std::process::id()))
    }

    #[test]
    fn writes_pack_mcmeta_with_the_pack_format() {
        let dir = temp_dir("pack-mcmeta");""",
)

s = s.replace(
    """    #[test]
    fn writes_mipmap_levels_and_supported_formats() {
        let dir = std::env::temp_dir().join(format!("xbrstudio-pack-{}", std::process::id()));""",
    """    #[test]
    fn writes_mipmap_levels_and_supported_formats() {
        let dir = temp_dir("pack-mipmap");""",
)

s = s.replace(
    """    #[test]
    fn zips_a_directory() {
        let dir = std::env::temp_dir().join(format!("xbrstudio-zip-{}", std::process::id()));""",
    """    #[test]
    fn zips_a_directory() {
        let dir = temp_dir("pack-zip");""",
)

open(p, "w").write(s)
print("pack tests use unique directories")
