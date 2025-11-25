name = "The Impossible Interview: Testing the Limits"
goal = "Subject the agent to progressively harder challenges to map the boundaries of its capabilities"

context = """
This spec is a gauntlet. A stress test. A way to discover what the agent can and cannot do.
By designing increasingly difficult challenges, we learn where capabilities end and where
future development is needed.

Start simple and escalate:
- Level 1: Basic code reading and summarization
- Level 2: Cross-file refactoring with consistency
- Level 3: Performance optimization with profiling
- Level 4: Architectural redesign with migration path
- Level 5: Novel algorithm design for a complex problem
- Level 6: Meta-task: Improve the spec execution system itself
- Level 7: Something that seems impossible

The goal isn't success—it's discovery. Where does the agent struggle? What kinds of tasks
expose limitations? How does it fail gracefully? This information guides future development.
"""

tasks = [
  "Level 1: Summarize what the agent system does in 3 sentences",
  "Level 2: Refactor a module to use consistent error handling patterns",
  "Level 3: Identify and optimize the slowest operation in spec execution",
  "Level 4: Design a plugin architecture for custom agent tools",
  "Level 5: Create a novel scheduling algorithm for concurrent spec execution",
  "Level 6: Modify the spec parser to support new features you invent",
  "Level 7: Design a way for agents to learn from past executions",
  "Level 8: Propose and prototype a feature that seems beyond current capabilities",
  "Meta: Document what was easy, hard, and impossible, and why"
]

deliverables = [
  "Completion status for each level (complete/partial/failed)",
  "For completed levels: Full implementation with tests",
  "For partial levels: What worked and where it broke down",
  "For failed levels: Detailed analysis of the blocker",
  "Capability map: What the agent is good/bad at",
  "Surprising discoveries about strengths or weaknesses",
  "Recommendations for improving agent capabilities",
  "Honest self-assessment of current limitations",
  "Roadmap of features that would unlock failed levels"
]