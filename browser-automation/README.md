# Browser Automation for Vibe Kanban

TypeScript-based browser automation for integrating web-based chat agents (Claude Chat, M365 Copilot) with vibe-kanban tasks.

## Features

- **Manual Authentication**: Opens browser visibly for user login
- **Multiple Agent Support**: Claude Chat and M365 Copilot
- **Robust Selector Handling**: Multiple fallback selectors for UI changes
- **Real-time Streaming**: Streams responses as they arrive
- **TypeScript**: Fully typed for better development experience
- **Stealth Mode**: Anti-detection using puppeteer-extra-plugin-stealth

## Quick Start

```bash
# Install dependencies
npm install

# Build TypeScript
npm run build

# Run with ts-node (development)
npm run dev claude "Hello, can you help me with a task?"

# Run compiled JavaScript (production)
npm run start claude "Hello, can you help me with a task?"
```

## Usage

### Command Line Interface

```bash
# Claude Chat
ts-node src/browser-chat.ts claude "Your message here"

# M365 Copilot
ts-node src/browser-chat.ts m365 "Your message here"
```

### Supported Agents

- `claude`, `claude-chat`: Claude AI chat interface
- `m365`, `m365-copilot`, `copilot`: Microsoft 365 Copilot

### Environment Variables

- `LOG_LEVEL`: Set to `debug`, `info`, or `error` (default: `info`)

## Architecture

```
src/
├── agents/           # Agent-specific configurations
│   ├── claude-chat.ts
│   └── m365-copilot.ts
├── utils/            # Utility functions
│   └── logger.ts
├── types.ts          # TypeScript type definitions
└── browser-chat.ts   # Main automation class
```

## Integration with Vibe Kanban

This module is designed to be called from Rust executors in the main vibe-kanban application:

```rust
// From Rust executor
Command::new("node")
    .arg("browser-automation/dist/browser-chat.js")
    .arg("claude")
    .arg(&prompt)
    .spawn_async()
```

## Manual Login Workflow

1. Browser opens visibly to the specified chat service
2. User manually logs in through the browser UI
3. User presses Enter in terminal to continue
4. Automation sends message and streams response
5. Browser closes automatically when complete

## Error Handling

- Graceful browser cleanup on termination signals
- Multiple selector fallbacks for UI resilience  
- Timeout handling for unresponsive pages
- Detailed error logging

## Development

```bash
# Watch mode during development
npx tsc --watch

# Debug mode with verbose logging
LOG_LEVEL=debug npm run dev claude "test message"
```