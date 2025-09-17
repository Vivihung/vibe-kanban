/**
 * Jest test setup file
 * 
 * This file runs before each test suite and sets up the testing environment
 * for browser automation tests.
 */

import { spawn, ChildProcess } from 'child_process';
import fetch from 'node-fetch';
import { logger } from '../src/utils/logger';

// Test server instance
let testServer: ChildProcess | null = null;

// Global test configuration
export const TEST_CONFIG = {
  TEST_SERVER_PORT: 3030,
  TEST_SERVER_URL: 'http://127.0.0.1:3030',
  BROWSER_TIMEOUT: 30000, // 30 seconds
  RESPONSE_TIMEOUT: 10000, // 10 seconds for chat responses
};

// Test agent configurations that point to our test server
export const TEST_AGENTS = {
  claude: {
    name: 'Claude Chat',
    url: `${TEST_CONFIG.TEST_SERVER_URL}/claude-chat`,
    loginUrl: `${TEST_CONFIG.TEST_SERVER_URL}/claude-login`,
    inputSelectors: [
      '[data-testid="chat-input"]',
      '[contenteditable="true"]',
      'textarea[placeholder*="message"]',
      'textarea[placeholder*="Message"]'
    ],
    sendButtonSelectors: [
      '[data-testid="send-button"]',
      'button[type="submit"]',
      'button[aria-label*="Send"]'
    ]
  },
  m365: {
    name: 'M365 Copilot',
    url: `${TEST_CONFIG.TEST_SERVER_URL}/m365-chat`,
    loginUrl: `${TEST_CONFIG.TEST_SERVER_URL}/m365-login`,
    inputSelectors: [
      '[data-testid="chat-input"]',
      'textarea[placeholder*="Ask"]',
      'textarea[placeholder*="message"]',
      '[contenteditable="true"]'
    ],
    sendButtonSelectors: [
      '[data-testid="send-button"]',
      'button[type="submit"]',
      'button[aria-label*="Send"]'
    ]
  }
};

/**
 * Start the test server before running tests (only if not already running)
 */
export async function startTestServer(): Promise<void> {
  // Check if server is already running
  try {
    const response = await fetch(`${TEST_CONFIG.TEST_SERVER_URL}/api/status`);
    if (response.ok) {
      logger.info('Test server already running, skipping startup');
      return;
    }
  } catch (error) {
    // Server not running, continue with startup
  }
  
  return new Promise((resolve, reject) => {
    logger.info('Starting test server...');
    
    testServer = spawn('node', ['test/test-server.js'], {
      stdio: ['ignore', 'pipe', 'pipe'],
      cwd: process.cwd()
    });
    
    testServer.stdout?.on('data', (data) => {
      const output = data.toString();
      if (output.includes('Test server running')) {
        logger.info('Test server started successfully');
        resolve();
      }
    });
    
    testServer.stderr?.on('data', (data) => {
      logger.error('Test server error:', data.toString());
    });
    
    testServer.on('error', (error) => {
      logger.error('Failed to start test server:', error);
      reject(error);
    });
    
    testServer.on('exit', (code) => {
      if (code !== 0) {
        logger.error(`Test server exited with code ${code}`);
      }
    });
    
    // Timeout if server doesn't start within 10 seconds
    setTimeout(() => {
      reject(new Error('Test server startup timeout'));
    }, 10000);
  });
}

/**
 * Stop the test server after tests complete
 */
export async function stopTestServer(): Promise<void> {
  return new Promise((resolve) => {
    if (testServer) {
      logger.info('Stopping test server...');
      
      testServer.on('exit', () => {
        logger.info('Test server stopped');
        testServer = null;
        resolve();
      });
      
      testServer.kill('SIGTERM');
      
      // Force kill after 5 seconds if it doesn't stop gracefully
      setTimeout(() => {
        if (testServer) {
          testServer.kill('SIGKILL');
          testServer = null;
          resolve();
        }
      }, 5000);
    } else {
      resolve();
    }
  });
}

/**
 * Wait for a condition to be true with timeout
 */
export async function waitFor(
  condition: () => Promise<boolean> | boolean,
  timeout: number = 5000,
  interval: number = 100
): Promise<void> {
  const start = Date.now();
  
  while (Date.now() - start < timeout) {
    if (await condition()) {
      return;
    }
    await new Promise(resolve => setTimeout(resolve, interval));
  }
  
  throw new Error(`Condition not met within ${timeout}ms`);
}

/**
 * Sleep for a specified number of milliseconds
 */
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Global setup and teardown
beforeAll(async () => {
  await startTestServer();
}, 30000); // 30 second timeout for server startup

afterAll(async () => {
  await stopTestServer();
}, 10000); // 10 second timeout for server shutdown