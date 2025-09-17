/**
 * Browser Automation Behavior Tests
 * 
 * Tests the core browser automation behavior using mock pages
 * to validate expected workflow without external dependencies.
 */

import puppeteer, { Browser, Page } from 'puppeteer';
import { TEST_CONFIG, TEST_AGENTS, waitFor, sleep } from './setup';
import { logger } from '../src/utils/logger';

describe('Browser Automation Core Behavior', () => {
  let browser: Browser;
  let page: Page;
  
  beforeEach(async () => {
    browser = await puppeteer.launch({
      headless: true,
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

  test('should detect login required state', async () => {
    await page.goto(TEST_AGENTS.claude.loginUrl);
    
    const hasLoginForm = await page.$('[data-testid="login-form"]') !== null;
    const hasEmailInput = await page.$('input[type="email"]') !== null;
    const hasLoginButton = await page.$('[data-testid="login-button"]') !== null;
    const hasChatInput = await page.$('[data-testid="chat-input"]') !== null;
    
    expect(hasLoginForm).toBe(true);
    expect(hasEmailInput).toBe(true);
    expect(hasLoginButton).toBe(true);
    expect(hasChatInput).toBe(false);
    
    logger.info('✅ Login detection working correctly');
  });
  
  test('should detect logged-in state', async () => {
    await page.goto(TEST_AGENTS.claude.url);
    await page.waitForSelector('[data-testid="chat-input"]', { timeout: 5000 });
    
    const hasChatInput = await page.$('[data-testid="chat-input"]') !== null;
    const hasUserMenu = await page.$('[data-testid="user-menu"]') !== null;
    const hasLoginForm = await page.$('[data-testid="login-form"]') !== null;
    
    expect(hasChatInput).toBe(true);
    expect(hasUserMenu).toBe(true);
    expect(hasLoginForm).toBe(false);
    
    logger.info('✅ Chat interface detection working correctly');
  });
  
  test('should successfully send message and receive response', async () => {
    await page.goto(TEST_AGENTS.claude.url);
    await page.waitForSelector('[data-testid="chat-input"]');
    
    const testMessage = 'Test automation message';
    
    // Find and type message
    const messageInput = await page.$('[data-testid="chat-input"]');
    expect(messageInput).not.toBeNull();
    
    await messageInput!.click();
    await messageInput!.type(testMessage);
    
    // Send message
    await page.keyboard.press('Enter');
    
    // Wait for response
    await waitFor(async () => {
      const messages = await page.$$('[data-testid="message-content"]');
      return messages.length > 1; // Initial + user + response
    }, 10000);
    
    const messages = await page.$$('[data-testid="message-content"]');
    expect(messages.length).toBeGreaterThan(1);
    
    logger.info('✅ Message sending and response detection working correctly');
  });
  
  test('should simulate complete authentication flow', async () => {
    // Start at login page
    await page.goto(TEST_AGENTS.claude.loginUrl);
    await page.waitForSelector('[data-testid="login-button"]');
    
    // Verify login state
    let isLoginPage = await page.$('[data-testid="login-form"]') !== null;
    expect(isLoginPage).toBe(true);
    
    // Simulate login
    await page.click('[data-testid="login-button"]');
    await page.waitForNavigation({ waitUntil: 'networkidle2' });
    
    // Verify redirect to chat
    await page.waitForSelector('[data-testid="chat-input"]', { timeout: 10000 });
    const isChatPage = await page.$('[data-testid="chat-input"]') !== null;
    expect(isChatPage).toBe(true);
    
    // Verify login indicators are gone
    const hasLoginForm = await page.$('[data-testid="login-form"]') !== null;
    expect(hasLoginForm).toBe(false);
    
    logger.info('✅ Complete authentication flow working correctly');
  });
  
  test('should detect browser close events', (done) => {
    page.on('close', () => {
      logger.info('✅ Page close event detected correctly');
      done();
    });
    
    // Simulate page close
    page.close();
    
    setTimeout(() => {
      done(new Error('Page close event was not detected'));
    }, 2000);
  });
  
  test('should detect browser disconnect events', (done) => {
    browser.on('disconnected', () => {
      logger.info('✅ Browser disconnect event detected correctly');
      done();
    });
    
    // Simulate browser disconnect
    browser.close();
    
    setTimeout(() => {
      done(new Error('Browser disconnect event was not detected'));
    }, 2000);
  });
});

describe('Expected Automation Workflow Documentation', () => {
  test('should document complete expected workflow', async () => {
    /**
     * This test documents the complete expected behavior of browser automation:
     * 
     * 1. Browser launches with persistent profile
     * 2. Navigation to chat URL
     * 3. Login detection (automatic polling, no manual input)
     * 4. Message sending with multiple selector fallbacks
     * 5. Response detection with completion indicators
     * 6. Browser stays alive for follow-up messages
     * 7. Graceful cleanup on browser close events
     */
    
    const browser = await puppeteer.launch({
      headless: true,
      userDataDir: '/tmp/test-browser-automation-workflow',
      args: ['--no-sandbox', '--disable-setuid-sandbox']
    });
    
    try {
      const page = await browser.newPage();
      await page.setViewport({ width: 1366, height: 768 });
      
      // Step 1: Navigate and detect login state
      await page.goto(TEST_AGENTS.claude.loginUrl);
      const loginRequired = await page.$('[data-testid="login-form"]') !== null;
      expect(loginRequired).toBe(true);
      
      // Step 2: Complete login flow
      await page.click('[data-testid="login-button"]');
      await page.waitForNavigation({ waitUntil: 'networkidle2' });
      
      // Step 3: Verify chat interface
      await page.waitForSelector('[data-testid="chat-input"]');
      const chatAvailable = await page.$('[data-testid="chat-input"]') !== null;
      expect(chatAvailable).toBe(true);
      
      // Step 4: Send message
      const messageInput = await page.$('[data-testid="chat-input"]');
      await messageInput!.type('Automated workflow test message');
      await page.keyboard.press('Enter');
      
      // Step 5: Wait for response
      await waitFor(async () => {
        const messages = await page.$$('[data-testid="message-content"]');
        return messages.length > 1;
      }, 10000);
      
      // Step 6: Verify response received
      const messages = await page.$$('[data-testid="message-content"]');
      expect(messages.length).toBeGreaterThan(1);
      
      // Step 7: Browser remains open for follow-ups
      const isPageActive = !page.isClosed();
      expect(isPageActive).toBe(true);
      
      logger.info('✅ Complete automation workflow documented and validated');
      
    } finally {
      await browser.close();
    }
  });
});