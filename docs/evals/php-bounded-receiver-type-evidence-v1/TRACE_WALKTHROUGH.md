# Representative traces (rx cohort, rep 1)

Committed under `traces/`: six task shapes x three arms.

- `rx-job-handle` — parameter type hint (`JobService $job`)
- `rx-orders-handle` — declared property type hint (`private OrderService $orders`)
- `rx-pages-home` — constructor literal-new property write (`$this->pages = new PageRenderer()`)
- `rx-report-gen` — local literal-new (`$r = new ReportService()`)
- `rx-repo-save` — literal-new + typed-hint union path
- `nc-legacy-handle` — untyped receiver control (INDETERMINATE expected)

Each trace is a Devin session export with observable tool events only —
`exec` calls invoking `./repo_query`, `grep`, `find`, `read`/`edit`. No hidden
chain-of-thought. P1 = T0 binary (ad83425), P2 = T1 binary (receiver types).
