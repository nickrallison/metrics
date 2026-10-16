use std::{num::NonZeroU32, os::unix::fs::PermissionsExt};

use super::*;
use crate::{
    benchmark::{ArtifactPolicy, Benchmark, RunContext},
    benchmarks::mcm::{McmBenchmark, McmInput, McmOptions, PaperTool},
    build::Yosys,
    design::CellLibrary,
    history::CommitId,
    store::SqliteStore,
};

fn tool(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture() -> (tempfile::TempDir, PerfProfiler) {
    let root = tempfile::Builder::new()
        .prefix("perf with spaces ")
        .tempdir()
        .unwrap();
    let path = |name| root.path().join(name);
    let profiler = PerfProfiler {
        perf: path("perf"),
        perl: path("perl"),
        addr2line: path("addr2line"),
        stackcollapse: path("collapse.pl"),
        flamegraph: path("flamegraph.pl"),
    };
    tool(
        &profiler.perf,
        r#"#!/bin/sh
case "$1" in
record)
    shift
    while [ "$1" != -- ]; do
        if [ "$1" = -o ]; then shift; data=$1; fi
        shift
    done
    shift
    printf 'raw profile\n' > "$data"
    "$@" ;;
script)
    [ "${DEBUGINFOD_URLS-unset}" = '' ] || exit 9
    printf 'yosys;main;design.cc:12 1\n' ;;
esac
"#,
    );
    tool(&profiler.perl, "#!/bin/sh\nexec \"$@\"\n");
    tool(
        &profiler.stackcollapse,
        "#!/bin/sh\n[ \"$1\" = --srcline ] || exit 8\ncat\n",
    );
    tool(
        &profiler.flamegraph,
        "#!/bin/sh\nprintf '<svg>'\ncat\nprintf '</svg>'\n",
    );
    (root, profiler)
}

#[test]
fn addr2line_resolution_preserves_the_symlink_name_that_selects_its_mode() {
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("llvm-symbolizer");
    let link = root.path().join("llvm-addr2line-16");
    tool(&binary, "#!/bin/sh\n[ \"${0##*/}\" = llvm-addr2line-16 ]\n");
    std::os::unix::fs::symlink(binary, &link).unwrap();
    assert!(
        Command::new(resolve(&link).unwrap())
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn mcm_profiles_each_phase_after_all_samples_without_profiling_the_reference() {
    let (root, profiler) = fixture();
    let yosys = root.path().join("yosys");
    tool(
        &yosys,
        r#"#!/bin/sh
printf '%s\n' "$4" >> "$0.calls"
printf '=== unoptimized ===\n1 cells\n1 $add\n'
printf "Chip area for module 'unoptimized': 20\n"
"#,
    );
    let paper = root.path().join("paper");
    tool(
        &paper,
        "#!/bin/sh\nprintf 'called\\n' >> \"$0.calls\"\nprintf '// Cost: 1 adds/subtracts 1 shifts 0 negations\\n// Depth: 1\\n'\n",
    );
    let liberty = root.path().join("library.lib");
    fs::write(&liberty, "library(test) {}\n").unwrap();
    let benchmark = McmBenchmark {
        coefficients: vec![3],
        input_width: 32,
        library: CellLibrary {
            name: "test".into(),
            liberty,
        },
        options: McmOptions {
            max_depth: 16,
            search_budget: 1000,
            paper_seed: 1,
            paper_bit_width: 30,
        },
        repetitions: NonZeroU32::new(2).unwrap(),
    };
    let input = McmInput {
        yosys: Yosys {
            commit: CommitId("test".into()),
            executable: yosys.clone(),
            data_dir: root.path().into(),
        },
        paper: PaperTool {
            executable: paper.clone(),
        },
    };
    let workspace = root.path().join("artifacts");
    fs::create_dir(&workspace).unwrap();
    let mut store = SqliteStore::open(":memory:").unwrap();
    let mut context = RunContext::new(
        workspace.clone(),
        ArtifactPolicy {
            logs: false,
            scripts: false,
            netlists: false,
        },
        &mut store,
    );
    context.perf = Some(&profiler);
    let output = benchmark.run(&input, &mut context).unwrap();
    assert_eq!(output.yosys_mcm.values.len(), 2);
    assert_eq!(output.synth_without_mcm.values.len(), 2);
    assert_eq!(output.synth_with_mcm.values.len(), 2);
    assert_eq!(
        fs::read_to_string(paper.with_extension("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let calls = fs::read_to_string(yosys.with_extension("calls")).unwrap();
    assert_eq!(
        calls.lines().take(6).collect::<Vec<_>>(),
        [
            "mcm-0.ys",
            "mcm-1.ys",
            "baseline-0.ys",
            "baseline-1.ys",
            "with-mcm-0.ys",
            "with-mcm-1.ys"
        ]
    );
    assert_eq!(calls.lines().skip(6).collect::<Vec<_>>(), ["run.ys"; 6]);
    for phase in ["mcm", "baseline", "with-mcm"] {
        for index in 0..2 {
            let directory = workspace.join("perf").join(format!("{phase}-{index}"));
            assert!(directory.join("perf.data").is_file());
            assert!(directory.join("run.ys").is_file());
            assert!(directory.join("design.v").is_file());
            assert!(
                fs::read_to_string(directory.join("flamegraph.svg"))
                    .unwrap()
                    .contains("design.cc:12")
            );
            let commands = fs::read_to_string(directory.join("commands.log")).unwrap();
            for flag in [
                "dwarf,16384",
                "--addr2line=",
                "--no-inline",
                "+srcline",
                "--srcline",
            ] {
                assert!(commands.contains(flag), "missing {flag}");
            }
            assert!(fs::read_dir(directory).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".tmp")
            }));
        }
    }
}

#[test]
fn pipeline_failures_keep_raw_data_and_report_the_failing_stage() {
    for stage in ["record", "script", "collapse", "flamegraph"] {
        let (root, profiler) = fixture();
        let path = match stage {
            "record" | "script" => &profiler.perf,
            "collapse" => &profiler.stackcollapse,
            _ => &profiler.flamegraph,
        };
        let source = fs::read_to_string(path).unwrap();
        let failure = if stage == "record" || stage == "script" {
            format!(
                "#!/bin/sh\nif [ \"$1\" = {stage} ]; then echo deliberate-failure >&2; exit 5; fi\n{}",
                source.strip_prefix("#!/bin/sh\n").unwrap()
            )
        } else {
            "#!/bin/sh\necho deliberate-failure >&2\nexit 5\n".into()
        };
        tool(path, &failure);
        let directory = root.path().join("profile");
        let error = profiler
            .record(&Command::new("true"), &directory)
            .unwrap_err();
        assert!(
            matches!(error, Error::Profiling(detail) if detail.contains(stage) && detail.contains("deliberate-failure"))
        );
        assert!(directory.join(format!("{stage}.stderr.log")).is_file());
        if stage != "record" {
            assert!(directory.join("perf.data").is_file());
        }
        assert!(!directory.join("flamegraph.svg").exists() || stage == "flamegraph");
    }
}

#[test]
fn empty_stacks_keep_the_recording_and_do_not_produce_an_empty_svg() {
    let (root, profiler) = fixture();
    tool(&profiler.stackcollapse, "#!/bin/sh\nexit 0\n");
    let directory = root.path().join("profile");
    assert!(matches!(profiler.record(&Command::new("true"), &directory),
        Err(Error::Profiling(detail)) if detail.contains("no sampled stacks")));
    assert!(directory.join("perf.data").is_file());
    assert!(!directory.join("flamegraph.svg").exists());
}

#[test]
#[ignore = "requires perf permissions, LLVM addr2line, FlameGraph tools, and a C compiler"]
fn native_flamegraph_contains_source_lines() {
    let profiler = PerfProfiler::new("/usr/bin/llvm-addr2line-16").unwrap();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("spin.c");
    fs::write(&source, "volatile unsigned long value; int main(void) { for (unsigned long i=0;i<200000000;i++) value += i; return 0; }\n").unwrap();
    let binary = root.path().join("spin");
    assert!(
        Command::new("cc")
            .args(["-O2", "-g"])
            .arg(source)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let directory = root.path().join("profile");
    profiler.record(&Command::new(binary), &directory).unwrap();
    let svg = fs::read_to_string(directory.join("flamegraph.svg")).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("spin.c:1"));
}
