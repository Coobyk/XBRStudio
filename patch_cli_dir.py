#!/usr/bin/env python3
"""Let the CLI take a resource pack folder as well."""
import sys

p = "src/main.rs"
s = open(p).read()


def swap(old, new):
    global s
    if old not in s:
        print("MISSING:\n%s" % old[:400])
        sys.exit(1)
    if s.count(old) != 1:
        print("NOT UNIQUE (%d):\n%s" % (s.count(old), old[:400]))
        sys.exit(1)
    s = s.replace(old, new)


swap(
    """    /// Minecraft client jar: batch-upscale every texture into a resource-pack folder.
    #[arg(long)]
    jar: Option<PathBuf>,""",
    """    /// Minecraft client jar, resource pack zip or pack folder: batch-upscale
    /// every texture into a resource-pack folder.
    #[arg(long)]
    jar: Option<PathBuf>,""",
)

swap(
    """    let report = jar::upscale_jar(jar_path, &out_dir, &opts, |progress| match progress {""",
    """    let report = jar::upscale_source(jar_path, &out_dir, &opts, |progress| match progress {""",
)

swap(
    """    let out_dir = cli.output.clone().unwrap_or_else(|| {
        let stem = jar_path.file_stem().unwrap_or_default().to_string_lossy();
        let parent = jar_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{stem}_x{factor}"))
    });""",
    """    let out_dir = cli.output.clone().unwrap_or_else(|| {
        if jar_path.is_dir() {
            let stem = jar_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let parent = jar_path.parent().unwrap_or_else(|| Path::new("."));
            return parent.join(format!("{stem}_x{factor}"));
        }
        let stem = jar_path.file_stem().unwrap_or_default().to_string_lossy();
        let parent = jar_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!("{stem}_x{factor}"))
    });""",
)

open(p, "w").write(s)
print("main.rs takes a folder")
