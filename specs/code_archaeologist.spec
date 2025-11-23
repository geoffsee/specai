name = "Code Archaeologist: Excavating Digital History"
goal = "Unearth the story hidden in the git history and code evolution, treating the codebase as an archaeological site"

context = """
Every codebase is a layered history of decisions, pivots, and evolution. Like an archaeologist
carefully excavating ancient ruins, this spec tasks the agent with reconstructing the story
of spec-ai through its commit history, code patterns, and architectural decisions.

The agent should look for:
- Major architectural shifts and the reasons behind them
- Abandoned features or experiments still visible in the code
- Evolution of key abstractions and APIs
- The "fossils" of old approaches that inform current design
- Patterns that reveal the development philosophy and priorities
"""

tasks = [
  "Analyze the git history to identify major milestones and pivots",
  "Trace the evolution of core modules (config, persistence, agents)",
  "Identify commented-out code or TODO markers that reveal planned directions",
  "Find patterns in commit messages that reveal development philosophy",
  "Locate dependencies that were added and removed to understand experimentation",
  "Document how the project's scope and ambitions have evolved",
  "Discover any easter eggs, clever solutions, or hidden gems in the codebase"
]

deliverables = [
  "A narrative history of the spec-ai project timeline with key decision points",
  "Visual representation of architectural evolution (in markdown/ASCII art)",
  "Analysis of 3-5 'archaeological layers' in the code with examples",
  "List of abandoned or experimental features and why they didn't survive",
  "Insights about the development team's values based on code patterns",
  "Recommendations for documenting institutional knowledge before it's lost"
]