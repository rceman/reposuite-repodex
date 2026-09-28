# Byte budgets (serialized): lookup/def <=2KiB, rel/manifest/test <=4KiB,
path/flow <=8KiB, hard ceiling 12KiB. Cut at last complete line; emit
TRUNCATED_CONTINUATION only for relevant-but-truncated or over-budget — never
for merely-filtered irrelevant edges (FALSE_TRUNCATION_SIGNALS=0).
