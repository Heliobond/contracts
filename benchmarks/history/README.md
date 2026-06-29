# Performance History

The `Benchmarks` workflow uploads a JSON artifact for every run containing the
instruction counts emitted by tests named `bench_*`.

Use those artifacts to compare trend lines across pull requests and releases.
The fixed per-benchmark ceilings live in `.github/performance-budgets.json`.
