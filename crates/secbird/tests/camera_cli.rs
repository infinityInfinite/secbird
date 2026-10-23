use std::process::Command;

#[test]
fn help_exposes_camera_controls_but_not_native_resolution_experiments() {
    let output = Command::new(env!("CARGO_BIN_EXE_secbird"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("hdr on|off"));
    assert!(help.contains("fov wide|medium|narrow"));
    assert!(!help.contains("camera format"));
    assert!(!help.contains("EXPERIMENTAL CAMERA FORMAT"));
}
