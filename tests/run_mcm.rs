use std::process::Command;

#[test]
fn rejects_invalid_job_counts_before_starting_an_experiment() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["--jobs"],
        vec!["--jobs", "0"],
        vec!["--jobs", "-1"],
        vec!["--jobs", "abc"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_run_mcm"))
            .current_dir(directory.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--jobs requires a positive integer")
        );
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
