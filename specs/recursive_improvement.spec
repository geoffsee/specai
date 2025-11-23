name = "The Recursive Improvement Paradox"
goal = "Have the agent analyze and propose improvements to its own framework, creating a feedback loop of self-enhancement"

context = """
This spec challenges the agent to examine the very codebase that powers it. By analyzing
the spec-ai framework itself, the agent will identify optimization opportunities, architectural
improvements, and potential new features. This creates a fascinating loop where the tool
contemplates its own evolution.

The agent should consider performance, usability, extensibility, and developer experience.
Think deeply about what would make this framework more powerful while maintaining simplicity.
"""

tasks = [
  "Analyze the current architecture in src/ and identify design patterns used",
  "Review the configuration system and propose enhancements for agent profiles",
  "Examine the spec execution flow and suggest optimization opportunities",
  "Explore potential for new tool integrations or capabilities",
  "Consider how agents could collaborate or delegate to specialized sub-agents",
  "Identify opportunities for better error handling and user feedback",
  "Propose features that would make spec files more expressive and powerful"
]

deliverables = [
  "A detailed analysis of the current architecture's strengths and weaknesses",
  "At least 3 concrete proposals for framework improvements with justification",
  "A prioritized roadmap of enhancements from quick wins to ambitious features",
  "Example spec files demonstrating proposed new capabilities",
  "Reflection on the meta-nature of an AI improving its own substrate"
]