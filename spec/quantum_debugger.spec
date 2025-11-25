name = "Quantum Debugger: Exploring Parallel Possible Fixes"
goal = "Generate and evaluate multiple parallel solutions to a bug or problem, exploring the decision tree of possible fixes"

context = """
When debugging, developers typically try one fix at a time. But what if we could explore
multiple parallel universes of solutions simultaneously? This spec asks the agent to:

1. Identify a real or hypothetical bug in the codebase
2. Generate 3-5 completely different approaches to fixing it
3. Analyze the trade-offs of each approach
4. Recommend the optimal solution based on project values

This demonstrates multi-perspective reasoning and helps developers see options they might miss.
The "quantum" nature refers to holding multiple possibilities in superposition until observation
(human decision) collapses them to a single implementation.
"""

tasks = [
  "Identify a real bug, code smell, or improvement opportunity in the codebase",
  "Generate Solution A: The minimally invasive fix",
  "Generate Solution B: The elegant refactoring approach",
  "Generate Solution C: The performance-optimized solution",
  "Generate Solution D: The future-proof extensible solution",
  "Generate Solution E: The unconventional creative approach",
  "Analyze each solution's pros, cons, complexity, and maintainability",
  "Simulate the long-term consequences of each choice",
  "Recommend the best approach with clear reasoning"
]

deliverables = [
  "Clear statement of the problem with code examples",
  "5 distinct solution implementations with full code",
  "Comparison matrix evaluating each approach across multiple dimensions",
  "Time-series analysis showing how each solution ages over hypothetical feature additions",
  "Final recommendation with philosophical reflection on decision-making in software",
  "Meta-analysis of how the agent's own biases influenced the solution space"
]