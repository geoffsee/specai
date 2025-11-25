name = "Time Traveler: Predicting Future Maintenance Burden"
goal = "Analyze code to predict which parts will cause problems in the future and proactively address them"

context = """
Some code is a ticking time bomb. It works today but will cause headaches when requirements
change, scale increases, or team members rotate. This spec asks the agent to become a
fortune teller, predicting future pain points.

The agent should look for:
- Hard-coded assumptions that will break when conditions change
- Abstractions that will leak when extended
- Performance characteristics that won't scale
- API designs that will be difficult to evolve
- Dependencies that are unmaintained or likely to break
- Implicit coupling that makes changes risky
- Areas where current simplicity will become future complexity

This is preventive medicine for codebases. The goal is to fix tomorrow's problems today,
when the fixes are cheap and context is fresh.
"""

tasks = [
  "Scan the codebase for hard-coded values, magic numbers, and assumptions",
  "Identify modules with high coupling or hidden dependencies",
  "Analyze error handling for brittle patterns that fail unexpectedly",
  "Review configuration system for scalability and flexibility limits",
  "Examine database schema and migration strategy for evolution challenges",
  "Find performance bottlenecks that are fine now but won't scale",
  "Identify documentation gaps that will hurt future maintainers",
  "Predict which abstractions will leak when requirements expand",
  "Score each issue by likelihood and impact of future pain"
]

deliverables = [
  "Time-bomb report: Ranked list of code areas most likely to cause future problems",
  "For top 5 issues: Detailed analysis with specific line numbers and examples",
  "For each issue: Near-term, medium-term, and long-term risk scenarios",
  "Concrete refactoring recommendations with effort estimates",
  "Risk mitigation strategies for issues too expensive to fix now",
  "Technical debt tracking system integrated with existing tooling",
  "Reflection on the nature of technical debt and software entropy",
  "Guidelines for writing future-proof code in this codebase"
]