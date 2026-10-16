use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::*;
use crate::{
    history::CommitId,
    store::{History, HistoryQuery, RecordId, Reference, ReferenceCache, ReferenceId},
};

type Events = Rc<RefCell<Vec<String>>>;
type Workspaces = Rc<RefCell<Vec<PathBuf>>>;

#[derive(Clone, Serialize)]
enum Behavior {
    Succeed,
    Fail,
    Unsupported,
    Panic,
}

#[derive(Clone, Serialize)]
struct TestBenchmark {
    index: usize,
    behavior: Behavior,
    #[serde(skip)]
    events: Events,
    #[serde(skip)]
    workspaces: Workspaces,
}

impl Benchmark for TestBenchmark {
    type Input = Commit;
    type Output = usize;

    fn kind(&self) -> &'static str {
        "test"
    }
    fn version(&self) -> u32 {
        1
    }

    fn run<C: ReferenceCache>(
        &self,
        input: &Commit,
        context: &mut RunContext<'_, C>,
    ) -> Result<usize> {
        self.events
            .borrow_mut()
            .push(format!("run:{}:{}", input.id.0, self.index));
        assert!(context.workspace.is_dir());
        std::fs::write(context.workspace.join("probe"), b"sample")?;
        self.workspaces.borrow_mut().push(context.workspace.clone());
        match self.behavior {
            Behavior::Succeed => Ok(self.index),
            Behavior::Fail => Err(Error::NotImplemented("test benchmark failure")),
            Behavior::Unsupported => Err(Error::Unsupported("old tool".into())),
            Behavior::Panic => panic!("interrupted benchmark"),
        }
    }
}

#[derive(Serialize)]
struct TestSuite {
    benchmarks: Vec<TestBenchmark>,
    #[serde(skip)]
    generated: Cell<usize>,
    #[serde(skip)]
    fail_generation: bool,
    #[serde(skip)]
    fail_input: bool,
    #[serde(skip)]
    events: Events,
}

impl Suite for TestSuite {
    type Tool = Commit;
    type Benchmark = TestBenchmark;

    fn benchmarks(&self) -> Result<Vec<TestBenchmark>> {
        self.generated.set(self.generated.get() + 1);
        self.events.borrow_mut().push("freeze".into());
        if self.fail_generation {
            return Err(Error::NotImplemented("suite generation"));
        }
        Ok(self.benchmarks.clone())
    }

    fn input(&self, tool: &Commit) -> Result<Commit> {
        self.events
            .borrow_mut()
            .push(format!("input:{}", tool.id.0));
        if self.fail_input {
            return Err(Error::NotImplemented("input setup"));
        }
        Ok(tool.clone())
    }
}

struct TestBuilder {
    fail_commit: Option<String>,
    events: Events,
}

impl Builder for TestBuilder {
    type Tool = Commit;

    fn build(&mut self, commit: &Commit) -> Result<Commit> {
        self.events
            .borrow_mut()
            .push(format!("build:{}", commit.id.0));
        if self.fail_commit.as_ref() == Some(&commit.id.0) {
            return Err(Error::NotImplemented("test build failure"));
        }
        Ok(commit.clone())
    }
}

#[derive(Default)]
struct TestStore {
    definition: Option<ExperimentDefinition>,
    starts: Vec<RunStart>,
    finishes: Vec<(RecordId, Value)>,
    completed: Vec<CommitId>,
    build_failures: Vec<(CommitId, String)>,
    fail_start: bool,
    fail_finish: bool,
    events: Events,
}

impl ReferenceCache for TestStore {
    fn get<T: DeserializeOwned>(&self, _: &str) -> Result<Option<Reference<T>>> {
        Ok(None)
    }
    fn insert<T: Serialize>(&mut self, _: &str, _: &T) -> Result<ReferenceId> {
        Ok(1)
    }
}

impl Store for TestStore {
    fn register_experiment(&mut self, definition: &ExperimentDefinition) -> Result<ExperimentId> {
        self.events.borrow_mut().push("register".into());
        self.definition = Some(definition.clone());
        Ok(1)
    }

    fn start_run(&mut self, start: &RunStart) -> Result<RecordId> {
        if self.fail_start {
            return Err(Error::NotImplemented("test start failure"));
        }
        let id = self.starts.len() as RecordId + 1;
        self.events.borrow_mut().push(format!("start:{id}"));
        self.starts.push(start.clone());
        Ok(id)
    }

    fn finish_run<O: Serialize>(
        &mut self,
        record: RecordId,
        _: SystemTime,
        outcome: &Outcome<O>,
    ) -> Result<()> {
        if self.fail_finish {
            return Err(Error::NotImplemented("test finish failure"));
        }
        self.events.borrow_mut().push(format!("finish:{record}"));
        self.finishes.push((record, serde_json::to_value(outcome)?));
        Ok(())
    }

    fn record_build_failure(
        &mut self,
        _: ExperimentId,
        commit: &Commit,
        reason: &str,
    ) -> Result<()> {
        self.build_failures.push((commit.id.clone(), reason.into()));
        Ok(())
    }

    fn complete_commit(&mut self, _: ExperimentId, commit: &CommitId) -> Result<()> {
        for index in 0..self.definition.as_ref().unwrap().benchmarks.len() {
            assert!(self.starts.iter().enumerate().any(|(i, start)| {
                start.commit.id == *commit
                    && start.benchmark_index == index
                    && self.finishes.iter().any(|(id, _)| *id == i as RecordId + 1)
            }));
        }
        self.events
            .borrow_mut()
            .push(format!("complete:{}", commit.0));
        self.completed.push(commit.clone());
        Ok(())
    }

    fn history<O: DeserializeOwned>(&self, _: &HistoryQuery) -> Result<History<O>> {
        Err(Error::NotImplemented("unused test history"))
    }

    fn last_complete(&self, _: ExperimentId) -> Result<Option<CommitId>> {
        Ok(self.completed.last().cloned())
    }
}

struct Harness {
    experiment: Experiment<TestSuite>,
    builder: TestBuilder,
    store: TestStore,
    events: Events,
    workspaces: Workspaces,
}

impl Harness {
    fn new(behaviors: Vec<Behavior>) -> Self {
        let events = Events::default();
        let workspaces = Workspaces::default();
        let benchmarks = behaviors
            .into_iter()
            .enumerate()
            .map(|(index, behavior)| TestBenchmark {
                index,
                behavior,
                events: events.clone(),
                workspaces: workspaces.clone(),
            })
            .collect();
        Self {
            experiment: Experiment {
                history: BranchHistory {
                    repository: "unused".into(),
                    branch: "main".into(),
                },
                revisions: RevisionSelection::Explicit(vec![
                    CommitId("a".into()),
                    CommitId("b".into()),
                ]),
                suite: TestSuite {
                    benchmarks,
                    generated: Cell::new(0),
                    fail_generation: false,
                    fail_input: false,
                    events: events.clone(),
                },
                artifacts: ArtifactPolicy::default(),
            },
            builder: TestBuilder {
                fail_commit: None,
                events: events.clone(),
            },
            store: TestStore {
                events: events.clone(),
                ..TestStore::default()
            },
            events,
            workspaces,
        }
    }

    fn execute(&mut self) -> Result<()> {
        let benchmarks = self.experiment.suite.benchmarks()?;
        let commits = ["a", "b"]
            .into_iter()
            .map(|id| Commit {
                id: CommitId(id.into()),
                committed_at: UNIX_EPOCH,
                author: "author".into(),
                commit_message: id.into(),
            })
            .collect();
        run_resolved(
            &self.experiment,
            benchmarks,
            commits,
            &mut self.builder,
            &mut self.store,
        )
    }
}

#[test]
fn freezes_once_and_records_each_outcome_in_sequential_order() {
    let mut h = Harness::new(vec![
        Behavior::Succeed,
        Behavior::Fail,
        Behavior::Unsupported,
    ]);
    h.execute().unwrap();
    assert_eq!(h.experiment.suite.generated.get(), 1);
    assert_eq!(
        h.store.completed,
        vec![CommitId("a".into()), CommitId("b".into())]
    );
    assert_eq!(h.store.finishes.len(), 6);
    assert_eq!(h.store.finishes[0].1, serde_json::json!({"Succeeded": 0}));
    assert_eq!(
        h.store.finishes[1].1,
        serde_json::json!({"Failed": "not implemented: test benchmark failure"})
    );
    assert_eq!(
        h.store.finishes[2].1,
        serde_json::json!({"Unsupported": "old tool"})
    );
    let mut expected = vec!["freeze".to_string(), "register".into()];
    for (commit_index, commit) in ["a", "b"].into_iter().enumerate() {
        expected.extend([format!("build:{commit}"), format!("input:{commit}")]);
        for index in 0..3 {
            let id = commit_index * 3 + index + 1;
            expected.extend([
                format!("start:{id}"),
                format!("run:{commit}:{index}"),
                format!("finish:{id}"),
            ]);
        }
        expected.push(format!("complete:{commit}"));
    }
    assert_eq!(*h.events.borrow(), expected);
    let paths = h.workspaces.borrow();
    assert_eq!(paths.len(), 6);
    assert!(paths.iter().all(|path| !path.exists()));
    assert!(
        paths
            .iter()
            .enumerate()
            .all(|(i, path)| !paths[..i].contains(path))
    );
    assert!(h.store.starts.iter().all(|start| start.artifacts.is_none()));
    let definition = h.store.definition.as_ref().unwrap();
    assert_eq!(definition.benchmarks.len(), 3);
    assert_eq!(definition.commits[0].id.0, "a");
    assert_eq!(definition.commits[1].id.0, "b");
    assert_eq!(definition.benchmarks[0].parameters["index"], 0);
}

#[test]
fn records_build_failure_without_completing_that_commit() {
    let mut h = Harness::new(vec![Behavior::Succeed]);
    h.builder.fail_commit = Some("b".into());
    assert!(matches!(
        h.execute(),
        Err(Error::NotImplemented("test build failure"))
    ));
    assert_eq!(h.store.completed, vec![CommitId("a".into())]);
    assert_eq!(h.store.starts.len(), 1);
    assert_eq!(
        h.store.build_failures,
        vec![(
            CommitId("b".into()),
            "not implemented: test build failure".into()
        )]
    );
}

#[test]
fn start_failure_does_not_execute_a_benchmark() {
    let mut h = Harness::new(vec![Behavior::Succeed]);
    h.store.fail_start = true;
    assert!(matches!(
        h.execute(),
        Err(Error::NotImplemented("test start failure"))
    ));
    assert!(h.workspaces.borrow().is_empty());
    assert!(h.store.completed.is_empty());
}

#[test]
fn finalization_failure_leaves_an_unfinished_attempt_and_stops() {
    let mut h = Harness::new(vec![Behavior::Succeed, Behavior::Succeed]);
    h.store.fail_finish = true;
    assert!(matches!(
        h.execute(),
        Err(Error::NotImplemented("test finish failure"))
    ));
    assert_eq!(h.store.starts.len(), 1);
    assert!(h.store.finishes.is_empty());
    assert!(h.store.completed.is_empty());
    assert!(h.workspaces.borrow().iter().all(|path| !path.exists()));
}

#[test]
fn requested_artifacts_survive_and_are_linked_to_the_attempt() {
    let mut h = Harness::new(vec![Behavior::Succeed, Behavior::Fail]);
    h.experiment.artifacts.logs = true;
    h.execute().unwrap();
    for start in &h.store.starts {
        let path = start.artifacts.as_ref().unwrap();
        assert!(path.join("probe").is_file());
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn panic_keeps_the_started_attempt_unfinished() {
    let mut h = Harness::new(vec![Behavior::Panic]);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.execute())).is_err());
    assert_eq!(h.store.starts.len(), 1);
    assert!(h.store.finishes.is_empty());
    assert!(h.store.completed.is_empty());
    assert!(h.workspaces.borrow().iter().all(|path| !path.exists()));
}

#[test]
fn input_failure_leaves_commit_incomplete() {
    let mut h = Harness::new(vec![Behavior::Succeed]);
    h.experiment.suite.fail_input = true;
    assert!(matches!(
        h.execute(),
        Err(Error::NotImplemented("input setup"))
    ));
    assert!(h.store.starts.is_empty());
    assert!(h.store.completed.is_empty());
}

#[test]
fn suite_failure_stops_before_revision_resolution_or_registration() {
    let mut h = Harness::new(vec![Behavior::Succeed]);
    h.experiment.suite.fail_generation = true;
    assert!(matches!(
        run(&h.experiment, &mut h.builder, &mut h.store),
        Err(Error::NotImplemented("suite generation"))
    ));
    assert_eq!(*h.events.borrow(), vec!["freeze"]);
}

#[test]
fn experiment_records_real_sqlite_history_and_completion() {
    use crate::store::SqliteStore;

    let mut h = Harness::new(vec![
        Behavior::Succeed,
        Behavior::Fail,
        Behavior::Unsupported,
    ]);
    let mut store = SqliteStore::open(":memory:").unwrap();
    let benchmarks = h.experiment.suite.benchmarks().unwrap();
    let commits = ["a", "b"]
        .into_iter()
        .map(|id| Commit {
            id: CommitId(id.into()),
            committed_at: UNIX_EPOCH,
            author: "author".into(),
            commit_message: id.into(),
        })
        .collect();
    run_resolved(
        &h.experiment,
        benchmarks,
        commits,
        &mut h.builder,
        &mut store,
    )
    .unwrap();

    let history = store
        .history::<usize>(&HistoryQuery {
            experiment: 1,
            benchmark_kind: "test".into(),
            benchmark_version: 1,
            benchmark_parameters: None,
        })
        .unwrap();
    assert_eq!(history.records.len(), 6);
    assert!(matches!(
        history.records[0].record.outcome,
        Outcome::Succeeded(0)
    ));
    assert!(matches!(
        history.records[1].record.outcome,
        Outcome::Failed(_)
    ));
    assert!(matches!(
        history.records[2].record.outcome,
        Outcome::Unsupported(_)
    ));
    assert_eq!(store.last_complete(1).unwrap(), Some(CommitId("b".into())));
    assert!(store.unfinished_runs(1).unwrap().is_empty());
}
