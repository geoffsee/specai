name = "SpecAI smoke test"
goal = "Verify the SpecAI CLI can ingest and execute structured specs using the mock provider"

context = """
This spec should run quickly and deterministically. It is intended for automated or manual
sanity checks after code changes. Keep it simple so it can succeed without external tools.
"""

tasks = [
  "Confirm that the CLI recognizes the spec structure",
  "Summarize the goal and context in the assistant response",
  "Report any unexpected errors encountered during execution"
]

deliverables = [
  "Assistant response referencing the smoke test goal"
]
