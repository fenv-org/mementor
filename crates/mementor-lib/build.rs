use std::env;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor_src = manifest_dir.join("../../vendor/sqlite-vector/src");
    let vendor_libs = manifest_dir.join("../../vendor/sqlite-vector/libs");

    // Get the SQLite header directory from rusqlite's bundled build.
    // DEP_SQLITE3_INCLUDE is set by libsqlite3-sys via cargo:include=.
    // Fallback: search the cargo registry for the bundled sqlite3 header.
    let sqlite_include = env::var("DEP_SQLITE3_INCLUDE").unwrap_or_else(|_| {
        eprintln!("DEP_SQLITE3_INCLUDE not set, searching cargo registry...");

        // Print all DEP_ vars for debugging
        for (key, value) in env::vars() {
            if key.starts_with("DEP_") {
                eprintln!("  {key} = {value}");
            }
        }

        let cargo_home = env::var("CARGO_HOME")
            .unwrap_or_else(|_| format!("{}/.cargo", env::var("HOME").unwrap()));
        eprintln!("CARGO_HOME resolved to: {cargo_home}");

        let registry_src = PathBuf::from(&cargo_home).join("registry/src");
        eprintln!("registry/src exists: {}", registry_src.is_dir());

        if registry_src.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&registry_src) {
                for entry in entries.flatten() {
                    eprintln!("  index dir: {}", entry.path().display());
                }
            }
        }

        find_sqlite3_include(&registry_src).expect(
            "Could not find sqlite3.h — ensure libsqlite3-sys with bundled feature is a dependency",
        )
    });

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    assert!(
        target_os == "macos",
        "Milestone 1 only supports macOS. Target OS: {target_os}"
    );

    // Common files: sqlite-vector.c + distance-cpu.c
    let mut common = cc::Build::new();
    common
        .file(vendor_src.join("sqlite-vector.c"))
        .file(vendor_src.join("distance-cpu.c"))
        .include(&vendor_src)
        .include(&vendor_libs)
        .include(&sqlite_include)
        .define("SQLITE_CORE", None)
        .warnings(false)
        .opt_level(2);

    match target_arch.as_str() {
        "aarch64" => {
            // Apple Silicon: NEON is baseline, no extra flags needed.
            common.file(vendor_src.join("distance-neon.c"));
            common.compile("sqlite_vector");
        }
        "x86_64" => {
            // Intel Mac: SSE2 is baseline on all x86_64 CPUs.
            // AVX2/AVX512 are excluded for Milestone 1 — Apple Clang does not
            // support AVX intrinsics as compile-time constant initializers in C
            // mode, which sqlite-vector's distance-avx2.c requires.
            // Provide stubs for the missing init functions that distance-cpu.c
            // references at runtime.
            // Provide stubs that fall back to SSE2 for the missing
            // AVX2/AVX512 init functions. At runtime, if the CPU supports
            // AVX2 (or AVX512), distance-cpu.c calls init_distance_functions_avx2
            // (or avx512) which would skip SSE2 init entirely. Our stubs
            // redirect to SSE2 so the search still works correctly.
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            let stub_path = out_dir.join("distance_stubs.c");
            let mut stub = std::fs::File::create(&stub_path).unwrap();
            writeln!(stub, "extern void init_distance_functions_sse2(void);").unwrap();
            writeln!(
                stub,
                "void init_distance_functions_avx2(void) {{ init_distance_functions_sse2(); }}"
            )
            .unwrap();
            writeln!(
                stub,
                "void init_distance_functions_avx512(void) {{ init_distance_functions_sse2(); }}"
            )
            .unwrap();

            common.file(vendor_src.join("distance-sse2.c"));
            common.file(&stub_path);
            common.compile("sqlite_vector");
        }
        _ => {
            panic!("Unsupported architecture: {target_arch}");
        }
    }

    println!("cargo::rerun-if-changed=../../vendor/sqlite-vector/src");
    println!("cargo::rerun-if-changed=../../vendor/sqlite-vector/libs/fp16");
}

/// Search the cargo registry for the libsqlite3-sys bundled sqlite3 header directory.
fn find_sqlite3_include(registry_src: &std::path::Path) -> Option<String> {
    if !registry_src.is_dir() {
        return None;
    }
    // registry/src/<index-hash>/libsqlite3-sys-<version>/sqlite3/sqlite3.h
    for index_entry in std::fs::read_dir(registry_src).ok()? {
        let index_dir = index_entry.ok()?.path();
        for crate_entry in std::fs::read_dir(&index_dir).ok()?.flatten() {
            let crate_dir = crate_entry.path();
            if crate_dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libsqlite3-sys-"))
            {
                let sqlite3_dir = crate_dir.join("sqlite3");
                if sqlite3_dir.join("sqlite3.h").exists() {
                    return Some(sqlite3_dir.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}
