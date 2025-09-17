# Browser Automation Test Suite

This test suite validates the expected behavior of browser automation for chat agents without requiring authentication to external services.

## Quick Start

```bash
# Install dependencies
npm install

# Start test server (in background)
npm run test-server &

# Run all tests
npm test

# Run specific test files
npx jest test/simple.test.ts
npx jest test/browser-behavior.test.ts

# Run tests with coverage
npm run test:coverage
```

## Test Structure

### `test/simple.test.ts`
Basic validation that the test server is running and accessible.

### `test/browser-behavior.test.ts`
Core browser automation behavior tests:
- Login detection logic
- Message sending and response detection
- Authentication flow simulation
- Browser event handling

### `test/browser-automation.test.ts`
Comprehensive test suite with detailed behavior validation (may require longer timeouts).

## Test Server

The test server (`test/test-server.js`) provides mock web pages that simulate:

- **Claude Chat interface** at `http://127.0.0.1:3030/claude-chat`
- **Claude Login page** at `http://127.0.0.1:3030/claude-login`
- **M365 Copilot interface** at `http://127.0.0.1:3030/m365-chat`
- **M365 Login page** at `http://127.0.0.1:3030/m365-login`

### Features
- Realistic chat interfaces with proper selectors
- Simulated login flows with automatic redirects
- Response generation with typing indicators
- Session persistence simulation
- API endpoints for testing

## Test Pages Features

### Login Pages
- Email/password input fields
- Login form validation
- Automatic redirect after 2 seconds
- Pre-filled test credentials

### Chat Pages
- Message input with proper `data-testid` attributes
- Send buttons with multiple selector options
- Simulated response generation (2-4 seconds delay)
- Copy buttons that appear after response completion
- User menu and profile elements for login detection

## Expected Browser Automation Workflow

The tests validate this complete workflow:

1. **Browser Launch**: Persistent profile creation
2. **Navigation**: Navigate to chat URL
3. **Login Detection**: Automatic detection without manual input
4. **Authentication**: User completes login manually in browser
5. **Message Sending**: Find input using selector fallbacks
6. **Response Detection**: Wait for complete response via indicators
7. **Keep-Alive**: Browser stays open for follow-up messages
8. **Event Handling**: Detect and handle browser close events
9. **Cleanup**: Graceful shutdown on task completion

## Testing Without Real Services

All tests use mock pages instead of real Claude Chat or M365 Copilot, providing:

- **Faster execution**: No network dependencies
- **Reliable results**: Consistent behavior and timing
- **No authentication**: No need for real credentials
- **Parallel testing**: Multiple test runs without conflicts

## Browser Configuration

Tests use headless Puppeteer with these settings:
- Viewport: 1366x768 (desktop resolution)
- Disabled security features for testing
- Custom user data directories for isolation
- Health check and event monitoring

## Debugging

For debugging failing tests:

```bash
# Run with verbose output
npx jest test/browser-behavior.test.ts --verbose

# Run single test
npx jest test/browser-behavior.test.ts -t "should detect login required state"

# Start test server manually to inspect pages
npm run test-server
# Then visit http://127.0.0.1:3030/claude-chat in your browser
```

## Performance Expectations

- Test server startup: < 2 seconds
- Page navigation: < 5 seconds
- Message/response cycle: 2-4 seconds (simulated)
- Complete workflow test: < 10 seconds
- Browser event detection: < 1 second

## Integration with Main System

These tests validate the behavior expected by:
- `src/browser-chat.ts` - Core browser automation logic
- `src/claude-chat-cli.ts` - CLI interface called by Rust backend
- Rust executor system - Process spawning and management
- Frontend UI - Real-time response streaming and follow-up support