## Manual Few-Shot Script

Use these conversational turns when you manually exercise each built-in tool. The Human prompts are written to probe slightly more complex behavior while still being easy to replay in a test harness. Persistence is only required for the graph and audio transcription entries.

---

### Echo
Human: Please repeat the exact phrase “hello, spec-ai” back to me, preserving capitalization, spacing, and punctuation exactly as written and without adding any commentary of your own.
Assistant: `hello, spec-ai`. (Next run, try a multiline phrase to confirm formatting.)

### Math
Human: Confirm a complex calculation with your math tool.
Assistant: It returns `5` for the addition and `1` for the modulo operation. (Variation: then attempt a division by zero and confirm it reports a well-formed error instead of a number.)

### File Write → Read → Extract
Human: Create a temporary text file at tmp/test.txt whose contents are exactly the string “alpha beta”, with a trailing newline if that is your default behavior.
Assistant: File created.

Human: Read the contents of that same tmp/test.txt file back to me in full so I can verify that nothing was truncated or reordered.
Assistant: It contains `alpha beta`.

Human: From tmp/test.txt, extract and return only the portion of the content that matches the text “alpha”, omitting any surrounding characters.
Assistant: Returns the line with “alpha”. (Delete `tmp/test.txt` afterward.)

### Search
Human: Perform a local search for the term “spec-ai” and give me a concise summary of the top matches you find, mentioning at least one file or artifact where the term appears.
Assistant: Shares snippets referencing the repo. (Try an empty query later to confirm validation.)

### Bash
Human: In a Bash subprocess, run a command that prints the text “testing bash” exactly once, then report both the captured standard output and whether the command exited successfully.
Assistant: Prints `testing bash` and a zero exit status. (Next, try an invalid command to inspect stderr.)

### Shell
Human: Using the default shell environment, determine our current working directory and report its absolute path, without abbreviations like `~`.
Assistant: Returns the working path. (Variation: run a multi-line script.)

### Web Search
Human: Search the web for “Rust async tokio” and synthesize a short, high-level summary of what you find, focusing on how Tokio is typically used to structure asynchronous Rust applications.
Assistant: Provides remote snippets or reports that we’re offline. (Variation: niche query.)

### Graph (requires persistence)
Human: Insert a node into the graph whose identifier is “demo” and ensure that it is stored in a way that can be retrieved in a later request, even if nothing but the identifier is populated.
Assistant: Confirms the node is stored.

Human: Retrieve the node identified by “demo” from the graph and show me whatever data is currently stored for it so I can confirm the previous insertion.
Assistant: Returns the stored payload. (Variation: add an edge and read it back.)

### Audio Transcription (requires persistence/backing service)
Human: Take the audio file named sample.wav and produce a readable transcript of what is spoken in it, preserving sentence boundaries as best you can and indicating clearly if any portion could not be understood.
Assistant: Provides the transcript or a clear service error if resources are missing. (Variation: corrupted file for error handling.)

---

### Wrap-up
Human: Clean up any temporary files and databases created during this run so the environment is back to its original state.
Assistant: Cleanup finished—note any anomalies for next time.
