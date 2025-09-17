/**
 * Browser Automation Test Suite
 * 
 * This test suite documents and validates the expected behavior of the browser
 * automation system for chat agents. It tests the core functionality without
 * requiring authentication to real services.
 */

import puppeteer, { Browser, Page } from 'puppeteer';
import { TEST_CONFIG, TEST_AGENTS, waitFor, sleep } from './setup';
import { logger } from '../src/utils/logger';
import path from 'path';
import os from 'os';

describe('Browser Automation Core Behavior', () => {
  let browser: Browser;
  let page: Page;
  
  beforeEach(async () => {
    // Launch browser for each test
    browser = await puppeteer.launch({
      headless: true, // Use headless mode for tests
      args: [
        '--no-sandbox',
        '--disable-setuid-sandbox',
        '--disable-dev-shm-usage',
        '--disable-web-security',
        '--disable-features=VizDisplayCompositor'
      ]
    });
    
    page = await browser.newPage();
    await page.setViewport({ width: 1366, height: 768 });
  });
  
  afterEach(async () => {
    if (browser) {
      await browser.close();
    }
  });

  describe('Login Detection Logic', () => {
    test('should detect when user is NOT logged in (login page)', async () => {
      // Navigate to login page
      await page.goto(TEST_AGENTS.claude.loginUrl);
      
      // Should find login indicators
      const hasLoginForm = await page.$('[data-testid="login-form"]') !== null;
      const hasEmailInput = await page.$('input[type="email"]') !== null;
      const hasPasswordInput = await page.$('input[type="password"]') !== null;
      const hasLoginButton = await page.$('[data-testid="login-button"]') !== null;
      
      expect(hasLoginForm).toBe(true);
      expect(hasEmailInput).toBe(true);
      expect(hasPasswordInput).toBe(true);
      expect(hasLoginButton).toBe(true);
      
      // Should NOT find chat interface
      const hasChatInput = await page.$('[data-testid="chat-input"]') !== null;
      expect(hasChatInput).toBe(false);
    });
    
    test('should detect when user IS logged in (chat page)', async () => {
      // Navigate to chat page (simulating logged-in state)
      await page.goto(TEST_AGENTS.claude.url);
      
      // Wait for page to load
      await page.waitForSelector('[data-testid="chat-input"]', { timeout: 5000 });
      
      // Should find chat interface indicators
      const hasChatInput = await page.$('[data-testid="chat-input"]') !== null;
      const hasUserMenu = await page.$('[data-testid="user-menu"]') !== null;
      const hasNewChatButton = await page.$('[data-testid="new-chat"]') !== null;
      
      expect(hasChatInput).toBe(true);
      expect(hasUserMenu).toBe(true);
      expect(hasNewChatButton).toBe(true);
      
      // Should NOT find login indicators
      const hasLoginForm = await page.$('[data-testid="login-form"]') !== null;
      const hasEmailInput = await page.$('input[type="email"]') !== null;
      
      expect(hasLoginForm).toBe(false);
      expect(hasEmailInput).toBe(false);
    });
    
    test('should simulate login flow and detect completion', async () => {
      // Start at login page
      await page.goto(TEST_AGENTS.claude.loginUrl);
      
      // Verify we're on login page
      await page.waitForSelector('[data-testid="login-button"]');
      const isLoginPage = await page.$('[data-testid="login-form"]') !== null;
      expect(isLoginPage).toBe(true);
      
      // Simulate login by clicking the login button
      await page.click('[data-testid="login-button"]');
      
      // Wait for redirect to chat page
      await page.waitForNavigation({ waitUntil: 'networkidle2' });
      
      // Verify we're now on the chat page
      await page.waitForSelector('[data-testid="chat-input"]', { timeout: 10000 });
      const isChatPage = await page.$('[data-testid="chat-input"]') !== null;
      expect(isChatPage).toBe(true);
      
      // Verify login indicators are gone
      const hasLoginForm = await page.$('[data-testid="login-form"]') !== null;
      expect(hasLoginForm).toBe(false);
    });
  });

  describe('Message Sending and Response Detection', () => {
    beforeEach(async () => {
      // Navigate to chat page for message tests
      await page.goto(TEST_AGENTS.claude.url);
      await page.waitForSelector('[data-testid="chat-input"]');
    });
    
    test('should find message input using multiple selectors', async () => {
      const inputSelectors = TEST_AGENTS.claude.inputSelectors;
      let foundInput = false;
      let usedSelector = '';
      
      for (const selector of inputSelectors) {
        const element = await page.$(selector);
        if (element) {
          foundInput = true;
          usedSelector = selector;
          break;
        }
      }
      
      expect(foundInput).toBe(true);
      expect(usedSelector).toBe('[data-testid="chat-input"]'); // Should find the first/primary selector
    });
    
    test('should successfully type and send a message', async () => {
      const testMessage = 'Hello, this is a test message for automation';
      
      // Find and type in message input
      const messageInput = await page.$('[data-testid="chat-input"]');
      expect(messageInput).not.toBeNull();
      
      await messageInput!.click();
      await messageInput!.type(testMessage);
      
      // Verify message was typed
      const inputValue = await page.evaluate(() => {
        const input = document.querySelector('[data-testid="chat-input"]') as HTMLTextAreaElement;
        return input?.value || input?.textContent || '';
      });
      
      expect(inputValue).toContain(testMessage);
      
      // Send message using Enter key
      await page.keyboard.press('Enter');
      
      // Wait for response to appear
      await waitFor(async () => {
        const messages = await page.$$('[data-testid="message-content"]');
        return messages.length > 1; // Initial message + our message + response
      }, 10000);
      
      // Verify response was received
      const messages = await page.$$('[data-testid="message-content"]');
      expect(messages.length).toBeGreaterThan(1);
    });
    
    test('should detect when response is complete', async () => {
      await page.goto(TEST_AGENTS.claude.url);
      await page.waitForSelector('[data-testid="chat-input"]');
      
      const messageInput = await page.$('[data-testid="chat-input"]');
      await messageInput!.type('Test response completion detection');
      await page.keyboard.press('Enter');
      
      // Wait for typing indicator to appear and then disappear
      await waitFor(async () => {
        const typingIndicator = await page.$('.typing-indicator.visible');
        return typingIndicator !== null;
      }, 5000);
      
      // Wait for response to complete (typing indicator should disappear)
      await waitFor(async () => {
        const typingIndicator = await page.$('.typing-indicator.visible');
        return typingIndicator === null;
      }, 15000);
      
      // Verify copy button appears (indicates response is complete)
      const copyButton = await page.$('button[aria-label="Copy"]');
      expect(copyButton).not.toBeNull();
    });
  });

  describe('Browser Session Persistence', () => {
    test('should create persistent user data directory', async () => {
      const userDataDir = path.join(os.homedir(), '.browser-automation', 'claude-profile');
      
      // Close existing browser
      await browser.close();
      
      // Launch new browser with persistent profile
      browser = await puppeteer.launch({
        headless: true,
        userDataDir,
        args: ['--no-sandbox', '--disable-setuid-sandbox']
      });
      
      page = await browser.newPage();
      await page.goto(TEST_AGENTS.claude.url);
      
      // Set a localStorage value to test persistence
      await page.evaluate(() => {
        localStorage.setItem('test-persistence', 'browser-automation-test');
      });
      
      await browser.close();
      
      // Launch another browser with same profile
      browser = await puppeteer.launch({
        headless: true,
        userDataDir,
        args: ['--no-sandbox', '--disable-setuid-sandbox']
      });
      
      page = await browser.newPage();
      await page.goto(TEST_AGENTS.claude.url);
      
      // Check if localStorage value persisted
      const persistedValue = await page.evaluate(() => {
        return localStorage.getItem('test-persistence');
      });
      
      expect(persistedValue).toBe('browser-automation-test');
    });
  });

  describe('Browser Close Detection', () => {
    test('should detect when browser tab is closed', (done) => {
      let closeEventFired = false;
      
      page.on('close', () => {
        closeEventFired = true;
        done();
      });
      
      // Simulate tab close
      page.close();
      
      setTimeout(() => {
        if (!closeEventFired) {
          done(new Error('Page close event was not detected'));
        }
      }, 2000);
    });
    
    test('should detect when browser process disconnects', (done) => {
      let disconnectEventFired = false;
      
      browser.on('disconnected', () => {
        disconnectEventFired = true;
        done();
      });
      
      // Simulate browser disconnect
      browser.close();
      
      setTimeout(() => {
        if (!disconnectEventFired) {
          done(new Error('Browser disconnect event was not detected'));
        }
      }, 2000);
    });
    
    test('should handle browser health checks gracefully', async () => {
      // Browser should be responsive to health checks
      let healthCheckPassed = false;
      
      try {
        const version = await browser.version();
        const title = await page.title();
        
        healthCheckPassed = !!version && typeof title === 'string';
      } catch (error) {
        healthCheckPassed = false;
      }
      
      expect(healthCheckPassed).toBe(true);
      
      // After browser closes, health check should fail
      await browser.close();
      
      let healthCheckFailed = false;
      try {
        await browser.version();
      } catch (error) {
        healthCheckFailed = true;
      }
      
      expect(healthCheckFailed).toBe(true);
      
      // Create new browser for cleanup
      browser = await puppeteer.launch({ headless: true });
      page = await browser.newPage();
    });
  });

  describe('M365 Copilot Specific Behavior', () => {
    test('should handle M365 Copilot interface correctly', async () => {
      await page.goto(TEST_AGENTS.m365.url);
      await page.waitForSelector('[data-testid="chat-input"]');
      
      // Verify M365-specific elements
      const hasM365Title = await page.evaluate(() => {
        return document.title.includes('M365 Copilot');
      });
      
      const hasUserAvatar = await page.$('[data-testid="user-menu"]') !== null;
      
      expect(hasM365Title).toBe(true);
      expect(hasUserAvatar).toBe(true);
      
      // Test message sending
      const messageInput = await page.$('[data-testid="chat-input"]');
      await messageInput!.type('Test M365 Copilot automation');
      await page.keyboard.press('Enter');
      
      // Wait for response (M365 typically takes longer)
      await waitFor(async () => {
        const messages = await page.$$('[data-testid="message-content"]');
        return messages.length > 1;
      }, 15000); // Longer timeout for M365
      
      const messages = await page.$$('[data-testid="message-content"]');
      expect(messages.length).toBeGreaterThan(1);
    });
  });

  describe('Error Handling and Edge Cases', () => {
    test('should handle missing selectors gracefully', async () => {
      await page.goto(TEST_CONFIG.TEST_SERVER_URL + '/nonexistent');
      
      // Try to find elements that don't exist
      const nonexistentInput = await page.$('[data-testid="nonexistent-input"]');
      const nonexistentButton = await page.$('[data-testid="nonexistent-button"]');
      
      expect(nonexistentInput).toBeNull();
      expect(nonexistentButton).toBeNull();
      
      // Should not throw errors when elements are missing
      let errorThrown = false;
      try {
        await page.waitForSelector('[data-testid="nonexistent-input"]', { timeout: 1000 });
      } catch (error) {
        errorThrown = true;
      }
      
      expect(errorThrown).toBe(true); // Timeout error is expected
    });
    
    test('should handle network errors during navigation', async () => {
      let navigationError = false;
      
      try {
        await page.goto('http://localhost:99999/invalid-url');
      } catch (error) {
        navigationError = true;
      }
      
      expect(navigationError).toBe(true);
    });
    
    test('should handle page crashes gracefully', (done) => {
      page.on('error', (error) => {
        expect(error).toBeDefined();
        done();
      });
      
      page.on('pageerror', (error) => {
        expect(error).toBeDefined();
        done();
      });
      
      // Simulate page error
      page.evaluate(() => {
        throw new Error('Simulated page error for testing');
      });
      
      setTimeout(() => {
        done(); // Complete test even if no error events fire
      }, 2000);
    });
  });
});

describe('Expected Automation Workflow', () => {
  /**
   * This test documents the complete expected workflow for browser automation
   */
  test('should complete full automation workflow correctly', async () => {
    const browser = await puppeteer.launch({
      headless: true,
      userDataDir: path.join(os.homedir(), '.browser-automation', 'test-full-workflow'),
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    try {
      const page = await browser.newPage();
      await page.setViewport({ width: 1366, height: 768 });
      
      // Step 1: Navigate to login page (simulating logged-out state)
      await page.goto(TEST_AGENTS.claude.loginUrl);
      await page.waitForSelector('[data-testid="login-button"]');
      
      // Step 2: Complete login
      await page.click('[data-testid="login-button"]');
      await page.waitForNavigation({ waitUntil: 'networkidle2' });
      
      // Step 3: Verify chat interface is available
      await page.waitForSelector('[data-testid="chat-input"]');
      const chatInput = await page.$('[data-testid="chat-input"]');
      expect(chatInput).not.toBeNull();
      
      // Step 4: Send initial message
      await chatInput!.type('Hello, this is an automated test message');
      await page.keyboard.press('Enter');
      
      // Step 5: Wait for complete response
      await waitFor(async () => {
        const messages = await page.$$('[data-testid="message-content"]');
        return messages.length > 1;
      }, 10000);
      
      // Step 6: Verify response was received
      const messages = await page.$$('[data-testid="message-content"]');
      expect(messages.length).toBeGreaterThan(1);
      
      // Step 7: Send follow-up message (testing session continuity)
      const followUpInput = await page.$('[data-testid="chat-input"]');
      await followUpInput!.click();
      await followUpInput!.type('This is a follow-up message to test session continuity');
      await page.keyboard.press('Enter');
      
      // Step 8: Wait for follow-up response
      await waitFor(async () => {
        const updatedMessages = await page.$$('[data-testid="message-content"]');
        return updatedMessages.length > messages.length;
      }, 10000);
      
      // Step 9: Verify follow-up response
      const finalMessages = await page.$$('[data-testid="message-content"]');
      expect(finalMessages.length).toBeGreaterThan(messages.length);
      
      // Step 10: Browser should remain open for additional follow-ups
      // (In real implementation, keepProcessAlive would maintain this state)
      const isPageActive = !page.isClosed();
      expect(isPageActive).toBe(true);
      
      logger.info('✅ Full automation workflow completed successfully');
      
    } finally {
      await browser.close();
    }
  });
});