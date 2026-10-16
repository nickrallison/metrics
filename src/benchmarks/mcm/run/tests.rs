use std::{num::NonZeroU32, os::unix::fs::PermissionsExt, time::Duration};

use super::*;

const NO_ARTIFACTS: ArtifactPolicy = ArtifactPolicy {
    logs: false,
    scripts: false,
    netlists: false,
};
use crate::{
    benchmark::{ArtifactPolicy, Benchmark},
    benchmarks::mcm::{McmOptions, PaperTool},
    build::Yosys,
    design::CellLibrary,
    history::CommitId,
    store::SqliteStore,
};

fn benchmark() -> McmBenchmark {
    McmBenchmark {
        coefficients: vec![3, -11],
        input_width: 32,
        library: CellLibrary {
            name: "test".into(),
            liberty: PathBuf::new(),
        },
        options: McmOptions {
            max_depth: 16,
            search_budget: 1000,
            paper_seed: 1,
            paper_bit_width: 30,
        },
        repetitions: NonZeroU32::new(2).unwrap(),
    }
}

fn tool(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

const YOSYS: &str = r#"#!/bin/sh
printf '%s\n' "$4" >> "$0.calls"
case "$4" in
baseline-*|inspect-baseline.ys) area=40 ;;
with-mcm-*|inspect-with-mcm.ys) area=20 ;;
*) area=0 ;;
esac
printf 'mcm: 2 constant multiplier(s) sharing one operand -> 1 adder(s), depth 2, cost 0\n'
printf '=== unoptimized ===\n 1 cells\n 1 $add\n'
if [ "$area" != 0 ]; then printf "Chip area for module 'unoptimized': %s\n" "$area"; fi
while IFS= read -r line; do
    case "$line" in
    'write_verilog '*)
        path=${line#write_verilog }; path=${path#\"}; path=${path%\"}
        printf 'module unoptimized; endmodule\n' > "$path" ;;
    esac
done < "$4"
"#;

const PAPER: &str = r#"#!/bin/sh
printf 'called\n' >> "$0.calls"
printf '// Cost: 1 adds/subtracts 2 shifts 0 negations\n// Depth: 2\n'
"#;

struct Fixture {
    root: tempfile::TempDir,
    benchmark: McmBenchmark,
    input: McmInput,
    store: SqliteStore,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let yosys = root.path().join("fake yosys");
        let paper = root.path().join("fake paper");
        tool(&yosys, YOSYS);
        tool(&paper, PAPER);
        let liberty = root.path().join("cell library.lib");
        fs::write(&liberty, "library(test) {}\n").unwrap();
        let mut benchmark = benchmark();
        benchmark.library.liberty = liberty;
        let input = McmInput {
            yosys: Yosys {
                commit: CommitId("first".into()),
                executable: yosys,
                data_dir: root.path().join("data"),
            },
            paper: PaperTool { executable: paper },
        };
        Self {
            root,
            benchmark,
            input,
            store: SqliteStore::open(":memory:").unwrap(),
        }
    }

    fn run(
        &mut self,
        scope: Option<&str>,
        artifacts: ArtifactPolicy,
    ) -> (McmOutput, tempfile::TempDir) {
        let workspace = tempfile::tempdir_in(self.root.path()).unwrap();
        let mut context =
            RunContext::new(workspace.path().to_path_buf(), artifacts, &mut self.store);
        context.reference_scope = scope.map(Into::into);
        (
            self.benchmark.run(&self.input, &mut context).unwrap(),
            workspace,
        )
    }

    fn paper_calls(&self) -> usize {
        fs::read_to_string(self.root.path().join("fake paper.calls"))
            .unwrap()
            .lines()
            .count()
    }
}

#[test]
fn complete_comparison_keeps_repetitions_and_reuses_only_compatible_references() {
    let mut fixture = Fixture::new();
    let (first, workspace) = fixture.run(Some("experiment:1"), NO_ARTIFACTS);
    assert_eq!(first.yosys_mcm.values.len(), 2);
    assert_eq!(first.paper.value.values.len(), 2);
    assert_eq!(first.synth_without_mcm.values.len(), 2);
    assert_eq!(first.synth_with_mcm.values.len(), 2);
    assert_eq!(first.synth_without_mcm.values[0].area, Some(40.0));
    assert_eq!(first.synth_with_mcm.values[0].area, Some(20.0));
    assert!(
        first
            .yosys_mcm
            .values
            .iter()
            .all(|value| value.wall_clock_time > Duration::ZERO)
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    fixture.input.yosys.commit.0 = "second".into();
    let (second, _) = fixture.run(Some("experiment:1"), NO_ARTIFACTS);
    assert_eq!(second.paper.id, first.paper.id);
    assert_eq!(second.paper.measured_at, first.paper.measured_at);
    assert_eq!(fixture.paper_calls(), 2);
    let calls = fs::read_to_string(fixture.root.path().join("fake yosys.calls")).unwrap();
    assert_eq!(calls.lines().count(), 12); // New Yosys samples at both commits.
    let (fresh, _) = fixture.run(Some("experiment:2"), NO_ARTIFACTS);
    assert_ne!(fresh.paper.id, first.paper.id);
    assert_eq!(fixture.paper_calls(), 4);
    // Changing executable content at the same path also invalidates the key.
    tool(
        &fixture.input.paper.executable,
        &format!("{PAPER}\n# changed executable\n"),
    );
    let (changed, _) = fixture.run(Some("experiment:2"), NO_ARTIFACTS);
    assert_ne!(changed.paper.id, fresh.paper.id);
    fixture.run(None, NO_ARTIFACTS);
    fixture.run(None, NO_ARTIFACTS);
    assert_eq!(fixture.paper_calls(), 10);
}

#[test]
fn retention_is_selective_and_diagnostic_netlists_add_no_samples() {
    let mut fixture = Fixture::new();
    let (_, workspace) = fixture.run(
        None,
        ArtifactPolicy {
            logs: true,
            ..NO_ARTIFACTS
        },
    );
    let logs: Vec<_> = fs::read_dir(workspace.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(logs.len(), 16); // Eight measured commands, stdout and stderr each.
    assert!(logs.iter().all(|path| path.extension().unwrap() == "log"));
    let (_, workspace) = fixture.run(
        None,
        ArtifactPolicy {
            scripts: true,
            ..NO_ARTIFACTS
        },
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 7);
    assert!(workspace.path().join("design.v").is_file());
    let (output, workspace) = fixture.run(
        None,
        ArtifactPolicy {
            netlists: true,
            ..NO_ARTIFACTS
        },
    );
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 3);
    for name in ["mcm.v", "baseline.v", "with-mcm.v"] {
        assert!(workspace.path().join(name).is_file());
    }
    assert_eq!(output.yosys_mcm.values.len(), 2);
    assert_eq!(output.synth_with_mcm.values.len(), 2);
}

#[test]
fn failed_processes_and_missing_passes_are_explicit_and_cleanup_scratch_files() {
    let mut fixture = Fixture::new();
    tool(
        &fixture.input.yosys.executable,
        "#!/bin/sh\necho deliberate-error >&2\nexit 9\n",
    );
    let workspace = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(
        workspace.path().into(),
        ArtifactPolicy {
            logs: true,
            ..NO_ARTIFACTS
        },
        &mut fixture.store,
    );
    let error = fixture
        .benchmark
        .run(&fixture.input, &mut context)
        .unwrap_err();
    assert!(matches!(&error, Error::Process { detail, .. } if detail.contains("deliberate-error")));
    assert!(workspace.path().join("process-000.stderr.log").is_file());
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 2);
    tool(
        &fixture.input.yosys.executable,
        "#!/bin/sh\necho 'ERROR: No such command: mcm' >&2\nexit 1\n",
    );
    assert!(matches!(
        fixture.benchmark.run(&fixture.input, &mut context),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn generator_preserves_order_sign_and_full_coefficient_and_product_widths() {
    let mut benchmark = benchmark();
    benchmark.coefficients = vec![3, -11, i32::MAX as i64, -i32::MAX as i64];
    validate(&benchmark).unwrap();
    let source = verilog(&benchmark).unwrap();
    assert!(source.contains("x * (3'sd3)"));
    assert!(source.contains("x * (-5'sd11)"));
    assert!(source.contains("x * (32'sd2147483647)"));
    assert!(source.contains("x * (-32'sd2147483647)"));
    assert!(source.contains("[63:0] y3"));
    benchmark.coefficients = vec![0];
    assert!(matches!(validate(&benchmark), Err(Error::Unsupported(_))));
    benchmark.coefficients = vec![i64::MAX];
    assert!(validate(&benchmark).is_err());
    benchmark.coefficients.clear();
    assert!(validate(&benchmark).is_err());
}

#[test]
fn parsers_scope_final_statistics_and_support_both_table_layouts() {
    let old = "=== unoptimized ===\nNumber of cells: 3\n 1 $add\n 2 $mul\n";
    let current = "mcm: 1 constant multiplier(s) -> 2 adder(s), depth 3, cost 0\n=== unoptimized ===\n3 - cells\n 1 - $add\n 1 - $sub\n 1 - $neg\nChip area for top module 'unoptimized': 1.25e2\n";
    let text = format!("{old}{current}");
    let stats = parse::yosys(&text, Duration::from_millis(5)).unwrap();
    assert_eq!(
        (
            stats.adds_subs,
            stats.negations,
            stats.multipliers,
            stats.depth
        ),
        (2, 1, 0, Some(3))
    );
    assert_eq!(parse::yosys(old, Duration::ZERO).unwrap().multipliers, 2);
    let synth = parse::synth(&text, Duration::ZERO).unwrap();
    assert_eq!(synth.cell_count, 3);
    assert_eq!(synth.area, Some(125.0));
    assert_eq!(parse::synth(old, Duration::ZERO).unwrap().area, None);
    let paper = parse::paper(
        "// Cost: 4 adds/subtracts 5 shifts 2 negations\n// Depth: 3\nt1 = cmul(t0, 3);\n",
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(
        (
            paper.adds_subs,
            paper.shifts,
            paper.negations,
            paper.multipliers,
            paper.depth
        ),
        (4, 5, 2, 1, 3)
    );
    for broken in [
        "",
        "=== unoptimized ===\n",
        "=== unoptimized ===\n2 cells\n1 $add\n",
    ] {
        assert!(parse::yosys(broken, Duration::ZERO).is_err());
    }
    assert!(
        parse::paper(
            "// Cost: 4 adds/subtracts 5 shifts 2 negations\n",
            Duration::ZERO
        )
        .is_err()
    );
    assert!(
        parse::synth(
            &format!("{old}Chip area for module 'unoptimized': NaN\n"),
            Duration::ZERO
        )
        .is_err()
    );
}

#[test]
#[ignore = "runs the real configured Yosys and paper tool"]
fn real_comparison_and_signed_arithmetic() {
    let repository = PathBuf::from("../yosys");
    let commit = crate::history::BranchHistory {
        repository: repository.clone(),
        branch: "main".into(),
    }
    .resolve(&crate::history::RevisionSelection::Explicit(vec![
        CommitId("HEAD".into()),
    ]))
    .unwrap()
    .remove(0);
    let mut builder = crate::build::YosysBuilder {
        repository,
        cache_dir: "output/builds/yosys-relwithdebinfo".into(),
        jobs: None,
    };
    let yosys = crate::build::Builder::build(&mut builder, &commit).unwrap();
    let mut benchmark = benchmark();
    benchmark.library.liberty =
        "../yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib".into();
    let input = McmInput {
        yosys,
        paper: PaperTool {
            executable: "../yosys/tests/techmap/mcm/synth/acm".into(),
        },
    };
    let workspace = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(":memory:").unwrap();
    let mut context = RunContext::new(workspace.path().into(), NO_ARTIFACTS, &mut store);
    let result = benchmark.run(&input, &mut context).unwrap();
    assert_eq!(result.yosys_mcm.values.len(), 2);
    assert!(result.synth_with_mcm.values[0].area.is_some());
    assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    context.artifacts = ArtifactPolicy {
        logs: true,
        scripts: true,
        netlists: true,
    };
    benchmark.run(&input, &mut context).unwrap();
    for name in ["mcm.v", "baseline.v", "with-mcm.v"] {
        assert!(workspace.path().join(name).is_file());
    }
    assert!(
        !fs::read_to_string(workspace.path().join("mcm-0.ys"))
            .unwrap()
            .contains("write_verilog")
    );
    benchmark.coefficients = vec![3, -11, i32::MAX as i64, -i32::MAX as i64];
    fs::write(
        workspace.path().join("design.v"),
        verilog(&benchmark).unwrap(),
    )
    .unwrap();
    for x in [i32::MIN, -1, 0, 1, i32::MAX] {
        let mut script =
            format!("read_verilog design.v; prep -top unoptimized; sat -verify -set x {x}");
        for (index, coefficient) in benchmark.coefficients.iter().enumerate() {
            let width =
                benchmark.input_width + (64 - coefficient.unsigned_abs().leading_zeros()) + 1;
            let expected = i128::from(x) * i128::from(*coefficient);
            let bits = expected & ((1i128 << width) - 1);
            script.push_str(&format!(" -prove y{index} {width}'h{bits:x}"));
        }
        let output = Command::new(&input.yosys.executable)
            .current_dir(workspace.path())
            .args(["-Q", "-T", "-p", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
