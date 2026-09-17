# Local model data

The `ollama/` directory is a local model volume for development and benchmark runs.
Model weights are intentionally excluded from Git and should not be bundled into the frontend.

Recommended local server configuration:

```text
OLLAMA_MODELS=E:\UI AI\ai-taskbar\model-data\ollama
OLLAMA_HOST=127.0.0.1:11435
```

The separate port keeps this project-scoped Ollama instance isolated from any existing Ollama installation on port 11434.
