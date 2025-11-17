## Manual Few-Shot Script

Use these natural conversational turns when you manually exercise each built-in tool. Persistence is only required for the graph and audio transcription entries.

---

### Echo
Human: Can you echo the phrase “hello, spec-ai” right back to me?
Assistant: `hello, spec-ai`. (Next run, try a multiline phrase to confirm formatting.)

### Math
Human: What’s the result of 2 plus 3?
Assistant: It returns `5`. (Variation: ask it to divide by zero to see error handling.)

### File Write → Read → Extract
Human: Drop a scratch file at tmp/test.txt containing the text “alpha beta”, please.
Assistant: File created.

Human: Great—read that file back to me.
Assistant: It contains `alpha beta`.

Human: Now pull any text that matches “alpha”.
Assistant: Returns the line with “alpha”. (Delete `tmp/test.txt` afterward.)

### Search
Human: Search locally for the term “spec-ai” and tell me what turns up.
Assistant: Shares snippets referencing the repo. (Try an empty query later to confirm validation.)

### Bash
Human: Run a simple command in Bash that prints “testing bash” and show me the output.
Assistant: Prints `testing bash` and a zero exit status. (Next, try an invalid command to inspect stderr.)

### Shell
Human: From the default shell, tell me which directory we’re currently in.
Assistant: Returns the working path. (Variation: run a multi-line script.)

### Web Search
Human: Look up “Rust async tokio” on the web and summarize anything interesting you find.
Assistant: Provides remote snippets or reports that we’re offline. (Variation: niche query.)

### Graph (requires persistence)
Human: Please add a graph node with the id “demo”.
Assistant: Confirms the node is stored.

Human: Now fetch the node “demo” so I can see it.
Assistant: Returns the stored payload. (Variation: add an edge and read it back.)

### Audio Transcription (requires persistence/backing service)
Human: Can you transcribe the audio file called sample.wav for me?
Assistant: Provides the transcript or a clear service error if resources are missing. (Variation: corrupted file for error handling.)

---

### Wrap-up
Human: I’m cleaning up scratch files and temporary databases now.
Assistant: Cleanup finished—note any anomalies for next time.
