use std::process::Command;

#[test]
fn help_and_invalid_options_exit_cleanly() {
    for arg in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_oxyspike"))
            .arg(arg)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("program.elf"));
    }
    for args in [
        vec![],
        vec!["--dtb"],
        vec!["--dump-memory"],
        vec!["--max-instructions"],
        vec!["--max-instructions", "-1"],
        vec!["--max-instructions", "18446744073709551616"],
        vec!["--unknown"],
        vec!["one", "two"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_oxyspike"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
}

#[cfg(unix)]
#[test]
fn unix_paths_preserve_non_utf8_bytes_and_option_terminator() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let dir = std::env::temp_dir().join(format!("oxyspike-cli-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let mut elf = [0u8; 64];
    elf[..6].copy_from_slice(b"\x7fELF\x02\x01");
    elf[18..20].copy_from_slice(&243u16.to_le_bytes());
    elf[24..32].copy_from_slice(&0x80000000u64.to_le_bytes());
    for name in [
        OsString::from_vec(b"guest-\xff.elf".to_vec()),
        OsString::from("-guest.elf"),
    ] {
        std::fs::write(dir.join(&name), elf).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_oxyspike"));
        command.current_dir(&dir).args(["--max-instructions", "0"]);
        if name.as_encoded_bytes().starts_with(b"-") {
            command.arg("--");
        }
        let output = command.arg(name).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Instruction limit reached"), "{stderr}");
        assert!(!stderr.contains("panicked"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_oxyspike"))
        .arg("--max-instructions")
        .arg(OsString::from_vec(vec![255]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid instruction limit"));
    std::fs::remove_dir_all(dir).unwrap();
}
