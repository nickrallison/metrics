# Todo

- Benchmarks
  - Large Benchmark Designs
    - Custom download instructions
    - Custom instructions to read into Yosys
    - Subset of valid cell libs
  - Synthetic Benchmarks
    - like my MCM benchmarks
    - small or generated
- Specific Performance Criteria
  - Area
  - Specific Cell Count - MCM
  - Runtime
  - WNS
- Scheduler
  - Every Commit?
  - Every Branch?
- SQLite Save results
  - **Reproducable** from the commit hash alone, runtime may be tricky
- Dataview
  - Benchmark Suite
  - Average v.s Specific Designs
  - Specific Cell Lib
  - View Logs
    - Either pass specific or generic log output
- Website UI
  - Display of the Dataview

# Hierarchy

- Design
  - Verilog
    - Location
      - Optional Download
      - File Location
    - Input Instructions
  - Generated
  - Configuration
    - TOML file
- Cell Libraries
- Run Target
  - Yosys
  - MCM had HCUB
