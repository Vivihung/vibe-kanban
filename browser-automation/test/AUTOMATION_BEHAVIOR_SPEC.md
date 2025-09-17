# Browser Automation Expected Behavior Specification

This document defines the expected behavior of the browser automation system for chat agents (Claude Chat, M365 Copilot). The behavior is validated through automated tests that use mock web pages instead of real services.

## Core Principles

1. **Browser Persistence**: Browser sessions remain open until task completion/deletion
2. **Automatic Login Detection**: No manual user input required during authentication
3. **Session Continuity**: Follow-up messages reuse existing browser sessions
4. **Graceful Cleanup**: Proper cleanup on browser close events
5. **Error Resilience**: Graceful handling of network issues and browser crashes

## Expected Workflow

### 1. Initial Task Creation
```
User creates browser chat task → Rust executor spawns Node.js CLI → CLI calls sendMessageToAgent()
```

### 2. Browser Launch and Authentication
```
Launch browser with persistent profile → Navigate to chat URL → Detect login status
  ├─ If logged in: Continue to message sending
  └─ If not logged in: Wait for user to complete manual login → Auto-detect completion
```

### 3. Message Sending
```
Find message input using selector fallbacks → Type message → Send via Enter key or button click → Stream response
```

### 4. Response Collection
```
Wait for response indicators → Detect completion via UI elements/stability → Output complete response
```

### 5. Keep-Alive Mode
```
Enter keepProcessAlive() → Monitor browser events → Stay open for follow-ups
  ├─ Browser close events → Graceful shutdown
  ├─ Process signals → Clean browser shutdown
  └─ Health checks → Detect browser availability
```

### 6. Follow-Up Messages
```
Reuse existing browser session → Send new message → Maintain conversation context
```

### 7. Task Completion
```
Task marked done/cancelled or deleted → Process terminates → Browser closes
```

## Login Detection Logic

### Not Logged In Indicators
- `input[type="email"]` - Email input field
- `input[type="password"]` - Password input field  
- `button:contains("Log in")` - Login buttons
- `[data-testid="login-form"]` - Login form containers
- URL contains `/login`, `/signin`, `/auth`

### Logged In Indicators
- `[data-testid="chat-input"]` - Chat input field
- `[data-testid="user-menu"]` - User profile menu
- `button:contains("New chat")` - New conversation buttons
- URL contains `/chat`, `/conversation`

### Auto-Detection Process
```
1. Check for login indicators (5-second timeout per selector)
2. If login indicators found → Return not logged in
3. Check for chat interface indicators (3-second timeout per selector) 
4. If chat indicators found → Return logged in
5. Check URL patterns for additional context
6. Default to not logged in for safety
```

## Message Input Detection

### Selector Priority (Claude Chat)
1. `[data-testid="chat-input"]` - Primary test selector
2. `[contenteditable="true"]` - Rich text editor
3. `textarea[placeholder*="message"]` - Message textareas
4. `.ProseMirror` - ProseMirror editor instances
5. `textarea` - Generic textarea fallback

### Selector Priority (M365 Copilot)  
1. `[data-testid="chat-input"]` - Primary test selector
2. `textarea[placeholder*="Ask"]` - Ask Copilot inputs
3. `textarea[placeholder*="message"]` - Message inputs
4. `[contenteditable="true"]` - Rich text editor fallback

## Response Detection Logic

### Completion Indicators (Claude)
- `button[aria-label="Copy"]` - Copy button appears when complete
- `[data-is-streaming="false"]` - Streaming state indicator
- Response text stability (unchanged for 3+ seconds)
- No typing indicator present

### Completion Indicators (M365)
- Response text stability (unchanged for 3+ seconds)
- No "thinking" indicator present
- UI interaction elements available

### Detection Process
```
1. Monitor response selectors for content changes
2. Track response length stability over time
3. Check for UI completion indicators
4. Wait additional 1 second after apparent completion
5. Return complete response text
```

## Browser Event Handling

### Close Detection Events
- `browser.on('disconnected')` - Browser process terminated
- `page.on('close')` - Tab/page closed by user
- `page.on('error')` - Page-level errors
- `page.on('pageerror')` - JavaScript errors

### Health Check Monitoring
```javascript
setInterval(async () => {
  try {
    await browser.version();  // Test browser connectivity
    await page.title();       // Test page accessibility
  } catch (error) {
    gracefulShutdown('Browser health check failed');
  }
}, 10000); // Every 10 seconds
```

### Graceful Shutdown Process
```
1. Detect shutdown trigger (event or health check failure)
2. Set isShuttingDown flag to prevent duplicate shutdowns
3. Log shutdown reason
4. Close browser cleanly if still available
5. Clear monitoring intervals
6. Exit process
```

## Session Persistence

### User Data Directory Structure
```
~/.browser-automation/
├── claude-profile/          # Claude Chat browser profile
│   ├── Default/
│   │   ├── Cookies         # Authentication cookies
│   │   ├── Local Storage/  # Session data
│   │   └── Preferences     # Browser preferences
└── m365-copilot-profile/   # M365 Copilot browser profile
    └── Default/
        ├── Cookies
        ├── Local Storage/
        └── Preferences
```

### Profile Isolation
- Separate profiles prevent authentication conflicts
- Each agent type uses dedicated user data directory
- Profiles persist across browser sessions
- Manual login only required once per profile

## Error Handling

### Network Issues
- Connection timeouts during navigation
- DNS resolution failures
- Server unavailability
- Retry logic with exponential backoff

### Browser Issues
- Browser launch failures
- Page load timeouts
- JavaScript execution errors
- Memory/resource constraints

### Selector Issues
- Missing UI elements
- Changed page structure
- Delayed element loading
- Multiple fallback selectors

### Recovery Strategies
1. **Selector Fallbacks**: Try multiple selectors for same functionality
2. **Timeout Handling**: Reasonable timeouts with error messages
3. **Graceful Degradation**: Continue with partial functionality when possible
4. **Clean Shutdown**: Always close browser resources on errors

## Performance Expectations

### Timing Requirements
- Browser launch: < 10 seconds
- Page navigation: < 30 seconds  
- Login detection: < 5 minutes maximum wait
- Message sending: < 5 seconds
- Response detection: < 2 minutes for complete response
- Health checks: Every 10 seconds

### Resource Management
- Single browser instance per task
- Automatic cleanup on process termination
- Memory usage monitoring
- CPU usage optimization

## Testing Strategy

### Mock Environment
- Local test server simulates chat interfaces
- No external dependencies during tests
- Predictable response timing and content
- All authentication states testable

### Test Coverage Areas
1. **Login Detection**: All login/logout states
2. **Message Sending**: All input methods and edge cases
3. **Response Detection**: Various response patterns and timing
4. **Session Persistence**: Profile creation and reuse
5. **Error Handling**: Network failures and browser issues
6. **Event Handling**: Close detection and cleanup
7. **End-to-End Workflow**: Complete automation cycles

### Test Data
- Standardized test messages and responses
- Consistent timing for response simulation
- Error injection for failure testing
- Session state variations

## Integration Points

### Rust Backend Integration
- Process spawning with correct arguments
- Session ID management for follow-ups
- Process lifecycle monitoring
- Error propagation and logging

### Frontend Integration  
- Real-time response streaming via SSE
- Task status updates during execution
- Follow-up message UI integration
- Error display and user feedback

### CLI Interface
- Command-line argument parsing
- Environment variable support
- Process signal handling
- Exit code management

## Security Considerations

### Authentication
- No credential storage in code
- Browser-managed authentication only
- Profile isolation between services
- Manual login requirement by design

### Process Security
- Sandboxed browser execution
- Limited filesystem access
- Network request monitoring
- Resource usage limits

## Maintenance and Monitoring

### Logging Strategy
- Structured logging with levels
- Performance metrics tracking
- Error rate monitoring
- User interaction analytics

### Health Monitoring
- Browser process health checks
- Response time tracking
- Success/failure rate metrics
- Resource utilization monitoring

### Debugging Support
- Screenshot capture on failures
- Browser console log collection
- Network request logging
- Step-by-step execution tracing

---

## Implementation Validation

The behavior described in this specification is validated through automated tests in `browser-automation.test.ts`. These tests use mock web pages that simulate real chat interfaces without requiring external authentication.

To run the tests:

```bash
npm install
npm test
```

To start the test server independently:

```bash
npm run test-server
```

This will start a local server at `http://127.0.0.1:3030` with test pages available at:
- `/claude-chat` - Claude Chat simulation
- `/claude-login` - Claude Login simulation  
- `/m365-chat` - M365 Copilot simulation
- `/m365-login` - M365 Login simulation