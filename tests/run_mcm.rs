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
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("error:"));
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn rejects_invalid_rerun_selectors_and_policies_before_opening_storage() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["--experiment", "0"],
        vec!["--experiment", "abc"],
        vec!["--workload", "-1"],
        vec!["--workload", "abc"],
        vec!["--artifacts", "bad"],
        vec!["--commit"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_run_mcm"))
            .current_dir(directory.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
