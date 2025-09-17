# DCR: Browser Automation Chat Agents for Vibe-Kanban (1-Day MVP)

## Overview

This simplified Design Change Request (DCR) outlines a minimal viable implementation of browser automation capabilities for vibe-kanban, enabling tasks to be dispatched to M365 Copilot (https://m365.cloud.microsoft.com/chat) and Claude Chat (https://claude.ai/) through basic Puppeteer automation. The system will stream responses back to kanban tasks similarly to existing coding agents.

**Time Constraint: 1 Day Implementation**
**Authentication: Manual user login (no automated auth)**

## Background

Vibe-kanban currently supports various AI coding agents that execute via CLI and stream responses through Server-Sent Events (SSE). This proposal adds a minimal browser-based chat agent that opens web interfaces and waits for manual user authentication before proceeding with chat automation.

## Simplified Architecture

### Minimal Browser Executor
- Add single `BrowserChat` variant to existing `CodingAgent` enum
- Implement basic `StandardCodingAgentExecutor` trait
- Reuse existing SSE streaming infrastructure
- Manual authentication workflow (no automated login)

## Minimal Implementation

### 1. Single Browser Executor

```rust
// In crates/executors/src/executors/mod.rs
pub enum CodingAgent {
    // ... existing variants
    BrowserChat, // New variant
}
```

### 2. Basic Browser Script

```rust
// crates/executors/src/executors/browser_chat.rs
pub struct BrowserChat;

#[async_trait]
impl StandardCodingAgentExecutor for BrowserChat {
    async fn spawn(&self, current_dir: &Path, prompt: &str) -> Result<AsyncGroupChild, ExecutorError> {
        // Launch simple Node.js script with Puppeteer
        let script_path = current_dir.join("browser_chat.js");
        Command::new("node")
            .arg(&script_path)
            .arg(prompt)
            .spawn_async()
    }
}
```

### 3. Browser Automation Script (Node.js)

```javascript
// browser_chat.js - Simple Puppeteer script
const puppeteer = require('puppeteer-extra');
const StealthPlugin = require('puppeteer-extra-plugin-stealth');
puppeteer.use(StealthPlugin());

async function main() {
  const prompt = process.argv[2];
  const browser = await puppeteer.launch({ headless: false }); // Visible for manual login
  const page = await browser.newPage();
  
  // Navigate to Claude Chat
  await page.goto('https://claude.ai/');
  
  // Wait for user to manually login
  console.log('Please login manually. Press Enter when ready to continue...');
  process.stdin.once('data', async () => {
    // Send message and stream response
    await sendMessageAndStream(page, prompt);
    await browser.close();
  });
}

async function sendMessageAndStream(page, message) {
  // Simple message sending and response extraction
  await page.type('[data-testid="chat-input"]', message);
  await page.click('[data-testid="send-button"]');
  
  // Stream response as it comes in
  while (true) {
    const response = await page.$eval('[data-testid="message-content"]:last-child', 
      el => el.textContent);
    console.log(response);
    await new Promise(r => setTimeout(r, 500));
  }
}

main();
```

### 4. Manual Authentication Workflow

- **Manual login**: Browser opens in visible mode
- **User interaction**: User manually logs into the service
- **Continuation signal**: Simple stdin input to continue automation
- **No credential storage**: No automated authentication complexity

### 5. Simple Streaming

- **Stdout streaming**: Node.js script outputs to stdout
- **Existing infrastructure**: Reuse current log streaming via `AsyncGroupChild`
- **No custom streaming**: Leverage existing SSE endpoints for process logs

### 6. Minimal Configuration

- **No complex config**: Use existing executor configuration patterns
- **Simple setup**: Just ensure Node.js and Puppeteer are available
- **No security complexity**: Manual auth eliminates credential storage needs

### 7. Basic Error Handling

- **Simple errors**: Standard process exit codes
- **Manual recovery**: User can retry if browser crashes
- **No complex resilience**: Keep it simple for 1-day implementation

### 8. No Optimization (MVP)

- **Single session**: One browser per task
- **No pooling**: Simple browser launch per execution
- **Basic cleanup**: Browser closes after task completion

## Minimal Dependencies

### System Requirements
- **Node.js**: For running Puppeteer script
- **npm packages**: `puppeteer-extra`, `puppeteer-extra-plugin-stealth`

### Setup Commands
```bash
npm init -y
npm install puppeteer-extra puppeteer-extra-plugin-stealth
```

### No New Rust Dependencies
- Reuse existing process spawning (`AsyncGroupChild`)
- Reuse existing streaming infrastructure

## Simplified Risks

### Acceptable Risks (for MVP)
- **Manual auth required**: User must login each time
- **UI changes**: May break selectors (acceptable for 1-day MVP)
- **Limited error handling**: Manual retry required
- **Single session**: No session persistence

### Mitigated Risks
- **Integration**: Reusing existing executor patterns
- **Streaming**: Leveraging existing infrastructure

## MVP Limitations (Acceptable)

- Manual login required for each session
- Browser runs in visible mode (no headless)
- Basic error handling only
- No session persistence
- May break if website UI changes
- Single concurrent browser session

## 1-Day Success Criteria

- Browser opens Claude Chat successfully
- User can manually login
- Script waits for login completion
- Message sent to chat interface
- Response streams back to vibe-kanban task
- Basic integration with existing task system

## Next Steps (Post-MVP)

- Automated authentication flows
- Session persistence
- Support for M365 Copilot
- Better error handling
- Headless mode option
- Multiple concurrent sessions

## Conclusion

This simplified DCR provides a minimal viable path to browser automation in vibe-kanban within a 1-day timeframe. By leveraging manual authentication and existing streaming infrastructure, we can quickly prototype browser-based chat agent integration.

**Key compromises for speed:**
- Manual login workflow
- No session persistence  
- Basic error handling
- Single browser instance
- Visible browser mode only

**Delivered value:**
- Proof of concept for browser automation
- Integration with existing task system
- Real-time response streaming
- Foundation for future enhancements