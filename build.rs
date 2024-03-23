//! # Build.rs
//!
//! Build script for embedding user program.
//! ---
//! Change log:
//!   - 2024/03/18: File created.

#![allow(unused)]

use std::process::Command;
use std::fs;
use std::io;
use io::Write;
use std::io::Stdout;
use std::path::Path;
use chrono::prelude::*;

static OUTPUT_FILE: &str = r"src/init/mod.rs";

const FILE_HEADER: &str = r#"// Generated Code, DO NOT MANUALLY MODIFIY.
"#;

fn create_binary(elf_path: &str) {
    Command::new("rust-objcopy").args(
        &["--strip-all", "-O", "binary",
            format!("{}", elf_path).as_str(),
            format!("{}.bin", elf_path).as_str()]
    ).status().unwrap();
}

fn create_disassembly(elf_path: &str) {
    let output = Command::new("rust-objdump").args(
        &["-S", "-C", format!("{}", elf_path).as_str()]
    ).output().unwrap();
    let mut file = fs::File::create(format!("{}.S", elf_path)).unwrap();
    file.write(&*output.stdout);
}

fn bundle_multiple_user_program<T: Write>(mut writer: T, target_path: &str) {
    println!("cargo:warning=[Build.rs] Bundling multiple user programs into kernel.");
    /* use objcopy to create raw binary */
    let count = fs::read_dir(target_path).unwrap().into_iter()
        .map(|p| {
            let p = p.unwrap();
            let filename = p.file_name().to_str().unwrap().to_string();
            if p.file_type().unwrap().is_file() && !filename.contains(".")
            {
                create_binary(format!("{}/{}", target_path, filename).as_str());
                create_disassembly(format!("{}/{}", target_path, filename).as_str());
                Some(format!("{}.bin", filename))
            } else {
                None
            }
        }).filter(|p| p.is_some()).map(|p| p.unwrap())
        .enumerate().map(|(i, path)| {
        println!("cargo:warning=[Build.rs] Bundling {}", path);
        writeln!(writer, r#"pub const PROG_{}_BINARY: &[u8] = include_bytes!("../../{}/{}");"#, i + 1, target_path, path);
        ()
    }).count();

    writeln!(writer, r#"
pub const PROG_BINARIES: [&[u8]; {}] = ["#, count);

    for i in 0..count {
        writeln!(writer, r#"    PROG_{}_BINARY, "#, i + 1);
    }

    writeln!(writer, r#"];"#);

    /* notify cargo to rerun this when... */
    // println!("cargo:rerun-if-changed={}", target_path);
}

fn bundle_init_user_program<T: Write>(mut writer: T, init_elf_path: &str) {
    // println!("cargo:rerun-if-changed={}", init_elf_path);
    create_binary(init_elf_path);
    create_disassembly(init_elf_path);
    writeln!(writer, r#"
pub const INIT_PROG_BINARY: &[u8] = include_bytes!("../../{}.bin");
"#, init_elf_path);
}


static INIT_FILE_PATH: &str = r"../user/target/riscv64gc-unknown-none-elf/debug/init";
static ELF_FILES_PATH: &str = r"../user/target/riscv64gc-unknown-none-elf/debug";

fn main() {

    let mut writer = fs::File::create(OUTPUT_FILE).unwrap();
    writeln!(writer, "{}", FILE_HEADER);
    writeln!(writer, "// Generated at {}", Local::now().format("%Y-%m-%d %H:%M:%S").to_string());

    // bundle_init_user_program(writer, INIT_FILE_PATH);
    bundle_multiple_user_program(writer, ELF_FILES_PATH);

    /* notify cargo to rerun this when... */
    println!("cargo:rerun-if-changed=../user/src");
    println!("cargo:rerun-if-changed=build.rs");
}
