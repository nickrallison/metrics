## High level idea

- I want a chronological view of a set of benchmarks across a period of time for a repos performace
- Then stage 2 would be an interface from which a nice graphical representation of that can be seen

## Current idea of hierarchy

- `Planner`
  - Returns a `Vec<Commit>`
- `Builder`
  - Takes a `Commit` and returns a `Yosys` object with the path of the newly built item
- `Suite` - unsure of what should be generic, maybe something `Suite<T> where T <-> Runner ...`
  - Returns a sweep of the inputs for a given `Runner`
  - Basically returns a `Vec<Runner::Input>` I guess?
- `Runner`
  - Takes a `Yosys` and a `Runner::Input` and returns a `Runner::Output`
    - `Runner::Output` should be the parsed results of the raw output, extracting cell count, area, wall time... and should be chosen by the specific implementation
- `Store`
  - Takes a `Commit`, a time, a `Runner::Input` and a `Runner::Output`, and adds it to a SQLite DB
  - provides `View` into the DB from which the next step formats into an output display
    - meant to provide an abstract interface for the last level of showing results to the user

### For Example

the user could define a `YosysSynthRunner` with `YosysSynthRunner::Input = (VerilogDesign, CellLibPath)`, `YosysSynthRunner::Output = (Runtime, Area)`

and the underlying script would look something like this:

```
let commands = {
    let mut commands = vec![];

    for file in verilog_design.files() {
        commands.push(format!("read -sv \"{}\"", file.display()));
    }

    commands.push(format!("hierarchy -top {}", verilog_design.top()));
    commands.push(format!("synth -top {} -noabc", verilog_design.top()));
    commands.push(format!(
        "dfflibmap -liberty \"{}\"",
        self.cell_lib().display()
    ));
    commands.push(format!("abc -liberty \"{}\"", &self.cell_lib().display()));
    commands.push("clean".into());
    commands.push(format!(
        "tee -o \"{}\" stat -liberty \"{}\"",
        stat_log_out.display(),
        &self.cell_lib.display()
    ));
    commands
};
```

or

```
read -sv "verilog/vexriscv.demo.GenFull.v"
hierarchy -top \vexriscv.demo.GenFull
synth -top \vexriscv.demo.GenFull -noabc
dfflibmap -liberty "/home/nick/Projects/yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib"
abc -liberty "/home/nick/Projects/yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib"
clean
tee -o "output/synth.stat" stat -liberty "/home/nick/Projects/yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib"
```

Then the user selects the set of designs they want to test on, as well as the cell libs they want to test on

Then they get back a ui with the history of the commits they ran on for each design

## thoughts

- I wonder if I should be yosys / commit abstract? might want to combine with openroad later, multiple chained runners? what is the right abstraction
  - is there an idiomatic / nice way to chain runners sequentially
- Ideally, I want to make the "user" only concerned with defining what a specific benchmark does, how to parse the results, how to serialize / deserialize with DB, and how to "Display" (unsure what this entails)
  - I think I'll want at least a graph with the specific Runner::Input Selectable, or averaged or ... in some sort of UI, either TUI or webui, idk yet, i just want a generic interface for either
- Prepping Designs
  - maybe reuse this format: benchmarks/mddbench/benchmarks/generated/mux/benchmarks.yaml
  - IDK if there's a nice way to handle it, just a prep script to be run before cargo? need to think on this
  - idk if VerilogDesign / &Design is the right abstraction
- Planner
  - What if I want more than just the main branch, maybe I want it enable for my dev branch, but just a specific kind of benchmark
    - e.g. I was working on a branch for optimizing multiplication by a constant, and created a synthetic benchmark set on which to run and compare against a _seperate tool_ as well as running with and without the new pass to see if it had a measurable effect vs just noise
- The end result is something like the restricted cartesian product of the input sets (restricted as in some constraints exist on the relations between the input items)
- What is the simplest way to benchmark performance, Yosys has some amount of internal performance monitoring, I can optionally use that to hook in and get more in-depth info
  - what about non-yosys tests,
