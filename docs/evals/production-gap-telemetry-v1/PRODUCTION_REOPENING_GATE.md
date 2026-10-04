# Production reopening gate (preregistered)

RepoDex static/research feature work MAY resume only when REAL production
telemetry (Relay-emitted AgentEvents, not benchmark replay) contains:

    >= 50 valid RepoDex-first sessions (artifact presentations observed)
    >= 20 native_discovery_after_artifact associations
    same gap family appears in >= 5 independent sessions
    same gap family occurs in >= 2 repositories OR >= 3 distinct investigations
    0 known instrumentation defects affecting that signal

AND the candidate feature is still weighed on: frequency, fallback
association, task-outcome association (where available), implementation
cost, truth-risk, existing fallback effectiveness — frequency alone is not
sufficient.

Until then:

    PHP_STATIC_EXPANSION_FROZEN      = true
    JEV_FROZEN                       = true
    CONTINUOUS_LEARNING_FROZEN       = true
