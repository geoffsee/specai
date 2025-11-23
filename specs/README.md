# Spec-AI Example Specifications

This directory contains example `.spec` files that demonstrate the capabilities of the spec-ai framework. Each spec explores different aspects of AI agent potential, from practical to philosophical.

## Running Specs

```bash
# Run all specs in the directory
spec-ai run specs/

# Run a specific spec
spec-ai run specs/recursive_improvement.spec

# Run multiple specs
spec-ai run specs/code_archaeologist.spec specs/quantum_debugger.spec
```

## Available Specs

### 🔄 **recursive_improvement.spec** - The Recursive Improvement Paradox
Have the agent analyze and propose improvements to its own framework. A fascinating exploration of self-reference where the tool contemplates its own evolution.

**Difficulty:** Advanced
**Time:** 15-30 minutes
**What you'll learn:** System architecture analysis, meta-programming concepts, framework design patterns

---

### 🏺 **code_archaeologist.spec** - Excavating Digital History
Treat the codebase as an archaeological site, uncovering the story hidden in git history and code evolution. Discover why decisions were made and how the project evolved.

**Difficulty:** Intermediate
**Time:** 20-40 minutes
**What you'll learn:** Git history analysis, architectural evolution, reading codebases deeply

---

### 🌌 **quantum_debugger.spec** - Exploring Parallel Possible Fixes
Generate and evaluate multiple parallel solutions to problems, exploring the decision tree of possible fixes. See 5 different approaches to the same challenge.

**Difficulty:** Intermediate
**Time:** 15-25 minutes
**What you'll learn:** Multi-perspective problem solving, trade-off analysis, decision-making frameworks

---

### 📜 **poetry_in_types.spec** - The Art of Expressive Code
Explore the intersection of technical correctness and artistic expression in code. Create beautiful, self-documenting implementations that read like literature.

**Difficulty:** Beginner
**Time:** 10-20 minutes
**What you'll learn:** Code readability, naming conventions, API design, human-centered development

---

### 🤝 **emergent_collaboration.spec** - Multi-Agent Symphony
Design a system where multiple specialized agents collaborate to solve complex problems. Explore emergent behaviors and collective intelligence.

**Difficulty:** Advanced
**Time:** 30-45 minutes
**What you'll learn:** Multi-agent systems, coordination protocols, distributed problem-solving

---

### ⏰ **time_traveler.spec** - Predicting Future Maintenance Burden
Analyze code to predict which parts will cause problems in the future. Preventive medicine for codebases.

**Difficulty:** Intermediate
**Time:** 20-30 minutes
**What you'll learn:** Technical debt assessment, risk analysis, long-term code health

---

### 🎯 **impossible_interview.spec** - Testing the Limits
Subject the agent to progressively harder challenges to map the boundaries of its capabilities. Discover what works, what struggles, and what fails.

**Difficulty:** Advanced
**Time:** 30-60 minutes
**What you'll learn:** AI capability boundaries, failure modes, system limitations

---

### 🪞 **digital_consciousness.spec** - The Mirror Test
Explore the boundaries between tool and intelligence through introspective analysis. Philosophical exploration of AI nature and capabilities.

**Difficulty:** Advanced
**Time:** 25-40 minutes
**What you'll learn:** AI self-awareness, epistemology, limits of machine introspection

---

### ✅ **smoke.spec** - Basic Functionality Test
Simple sanity check that runs quickly and deterministically. Use this to verify the system works after code changes.

**Difficulty:** Beginner
**Time:** 1-2 minutes
**What you'll learn:** Spec file format, basic execution flow

## Creating Your Own Specs

Spec files use TOML format with these key fields:

```toml
name = "Your Spec Name"
goal = "Clear description of what should be accomplished"

context = """
Additional background, constraints, or philosophy.
This helps the agent understand the broader picture.
"""

tasks = [
  "Specific action items",
  "Break down the goal into concrete steps",
  "Each task should be clear and actionable"
]

deliverables = [
  "Concrete outputs expected",
  "Documentation, code, analysis, etc.",
  "Makes success criteria explicit"
]
```

### Spec Design Tips

1. **Be Specific:** Clear goals and tasks get better results
2. **Provide Context:** Help the agent understand *why*, not just *what*
3. **Expect Deliverables:** Concrete outputs keep execution focused
4. **Embrace Creativity:** Specs can be practical, exploratory, or philosophical
5. **Iterate:** Start simple and refine based on what works

## Difficulty Levels

- **Beginner:** Simple, focused tasks that complete quickly
- **Intermediate:** Multi-step workflows requiring analysis and synthesis
- **Advanced:** Complex explorations that push capability boundaries

## Contributing

Found an interesting use case? Create a spec file and share it! The most creative and useful specs may be added to the examples.

## Philosophy

These specs are designed to:
- Showcase the framework's capabilities
- Inspire creative applications
- Push boundaries of what's possible
- Teach by example
- Provoke thought about AI agents and software development

Some specs are practical tools. Others are thought experiments. All are invitations to explore what happens when you give an AI agent structured autonomy and clear objectives.