#!/usr/bin/env node

import puppeteer from 'puppeteer-extra';
import StealthPlugin from 'puppeteer-extra-plugin-stealth';
import { Page, Browser } from 'puppeteer';
import { logger } from './utils/logger';
import * as path from 'path';
import * as os from 'os';

// Configure Puppeteer with stealth plugin
puppeteer.use(StealthPlugin());

interface AgentConfig {
  name: string;
  url: string;
  inputSelectors: string[];
  sendButtonSelectors: string[];
}

const agents: Record<string, AgentConfig> = {
  'claude': {
    name: 'Claude Chat',
    url: 'https://claude.ai/chat',
    inputSelectors: [
      '[contenteditable="true"]',
      'textarea[placeholder*="message"]',
      'textarea[placeholder*="Message"]', 
      'div[contenteditable="true"]',
      '[data-testid="chat-input"]',
      '.ProseMirror',
      'textarea'
    ],
    sendButtonSelectors: [
      'button[type="submit"]',
      'button[aria-label*="Send"]',
      '[data-testid="send-button"]'
    ]
  },
  'm365': {
    name: 'M365 Copilot',
    url: 'https://m365.cloud.microsoft.com/chat',
    inputSelectors: [
      'textarea[placeholder*="Ask"]',
      'textarea[placeholder*="message"]',
      '[contenteditable="true"]',
      '[data-testid="chat-input"]',
      'div[contenteditable="true"]',
      'textarea'
    ],
    sendButtonSelectors: [
      'button[type="submit"]',
      'button[aria-label*="Send"]',
      '[data-testid="send-button"]',
      '.send-button'
    ]
  }
};

async function waitForLogin(page: Page, agent: AgentConfig): Promise<void> {
  const maxWaitTime = 5 * 60 * 1000; // 5 minutes maximum wait
  const pollInterval = 2000; // Check every 2 seconds
  const startTime = Date.now();
  
  logger.info('Polling for login completion...');
  
  while (Date.now() - startTime < maxWaitTime) {
    try {
      // Check if login completed by looking for disappearance of login elements
      // and appearance of chat interface
      const isNowLoggedIn = await checkIfLoggedIn(page, agent);
      
      if (isNowLoggedIn) {
        logger.info('Login detected successfully!');
        return; // Login completed
      }
      
      // Log progress periodically
      const elapsed = Math.round((Date.now() - startTime) / 1000);
      if (elapsed % 30 === 0 && elapsed > 0) { // Every 30 seconds
        logger.info(`⏳ Still waiting for login... (${elapsed}s elapsed, ${Math.round(maxWaitTime/1000 - elapsed)}s remaining)`);
      }
      
      // Wait before next check
      await new Promise(resolve => setTimeout(resolve, pollInterval));
      
    } catch (error) {
      logger.debug('Error during login polling:', error);
      await new Promise(resolve => setTimeout(resolve, pollInterval));
    }
  }
  
  // Timeout reached
  throw new Error('Login timeout: User did not complete login within 5 minutes');
}

async function checkIfLoggedIn(page: Page, agent: AgentConfig): Promise<boolean> {
  try {
    // First, check for login indicators (sign that user is NOT logged in)
    const loginIndicators = [
      'button:contains("Log in")',
      'button:contains("Sign in")', 
      'button:contains("Login")',
      'button:contains("Sign In")',
      'input[type="email"]',
      'input[type="password"]',
      '[data-testid="login-form"]',
      '.login-form',
      'a[href*="login"]',
      'a[href*="signin"]',
      '[data-testid="login-button"]',
      '.auth-form',
      '[class*="login"]',
      '[class*="signin"]'
    ];

    // Use Promise.race to check multiple login indicators simultaneously
    const loginCheckPromises = loginIndicators.map(async (selector) => {
      try {
        const element = await page.waitForSelector(selector, { timeout: 1000 });
        if (element) {
          const isVisible = await element.isVisible();
          if (isVisible) {
            logger.debug(`Found login indicator: ${selector}`);
            return false; // Login form found, not logged in
          }
        }
      } catch (e) {
        // Selector not found, continue
      }
      return null;
    });

    const loginResults = await Promise.allSettled(loginCheckPromises);
    const foundLoginIndicator = loginResults.some(result => 
      result.status === 'fulfilled' && result.value === false
    );

    if (foundLoginIndicator) {
      return false; // Definitely not logged in
    }

    // Check for chat interface (sign that user IS logged in)
    const chatCheckPromises = agent.inputSelectors.map(async (selector) => {
      try {
        const element = await page.waitForSelector(selector, { timeout: 3000 });
        if (element) {
          const isVisible = await element.isVisible();
          if (isVisible) {
            logger.debug(`Found chat input: ${selector}`);
            return true; // Chat interface found, logged in
          }
        }
      } catch (e) {
        // Selector not found, continue
      }
      return null;
    });

    const chatResults = await Promise.allSettled(chatCheckPromises);
    const foundChatInterface = chatResults.some(result => 
      result.status === 'fulfilled' && result.value === true
    );

    if (foundChatInterface) {
      return true; // Definitely logged in
    }

    // Additional check: look for common post-login elements
    const postLoginIndicators = [
      '[data-testid="user-menu"]',
      '[data-testid="profile-button"]', 
      'button[aria-label*="Profile"]',
      '.user-avatar',
      '[class*="avatar"]',
      'button:contains("New chat")',
      '[data-testid="new-chat"]'
    ];

    for (const selector of postLoginIndicators) {
      try {
        const element = await page.$(selector);
        if (element && await element.isVisible()) {
          logger.debug(`Found post-login indicator: ${selector}`);
          return true;
        }
      } catch (e) {
        continue;
      }
    }

    // If no clear indicators either way, check the current URL for clues
    const currentUrl = page.url();
    if (currentUrl.includes('/login') || currentUrl.includes('/signin') || currentUrl.includes('/auth')) {
      logger.debug('URL indicates login page');
      return false;
    }

    if (currentUrl.includes('/chat') || currentUrl.includes('/conversation')) {
      logger.debug('URL indicates chat page, likely logged in');
      return true;
    }

    // If still unclear, assume not logged in for safety
    logger.debug('Login status unclear, assuming not logged in for safety');
    return false;
    
  } catch (error) {
    logger.debug('Error checking login status:', error);
    return false;
  }
}

async function sendMessageToAgent(agentName: string, message: string): Promise<void> {
  const agent = agents[agentName.toLowerCase()];
  if (!agent) {
    throw new Error(`Unknown agent: ${agentName}. Available: ${Object.keys(agents).join(', ')}`);
  }

  let browser: Browser | null = null;

  try {
    logger.info(`Initializing ${agent.name} automation...`);
    
    // Create persistent user data directory for this agent
    const userDataDir = path.join(os.homedir(), '.browser-automation', `${agentName.toLowerCase()}-profile`);
    
    browser = await puppeteer.launch({
      headless: false, // Always visible for manual login
      userDataDir, // Enable session persistence
      args: [
        '--no-sandbox',
        '--disable-setuid-sandbox',
        '--disable-dev-shm-usage',
        '--disable-accelerated-2d-canvas',
        '--no-first-run',
        '--no-zygote',
        '--disable-gpu'
      ]
    });

    const page: Page = await browser.newPage();

    // Set viewport and user agent to match your working code
    await page.setViewport({ width: 1366, height: 768 });
    await page.setUserAgent(
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36'
    );

    // Navigate to the agent
    logger.info(`Navigating to ${agent.name} at ${agent.url}...`);
    await page.goto(agent.url, {
      waitUntil: 'networkidle2',
      timeout: 120000 // 2 minutes
    });

    // Wait for page to load
    logger.info('Waiting for page to load...');
    await new Promise(resolve => setTimeout(resolve, 5000));

    // Check if already logged in by looking for chat interface
    const isLoggedIn = await checkIfLoggedIn(page, agent);
    
    if (!isLoggedIn) {
      // Automatic login detection - wait for user to complete login in browser
      logger.info('🔐 Login required: Please complete authentication in the browser window.');
      logger.info('⏳ Waiting for login to complete automatically...');
      logger.info('   (No manual input needed - login will be detected automatically)');
      
      await waitForLogin(page, agent);
      logger.info('✅ Login detected! Continuing with automation...');
    } else {
      logger.info('Already logged in, continuing with automation...');
    }

    logger.info('Continuing with automation...');

    // Find message input using multiple selectors
    logger.info('Looking for message input...');
    let messageInput = null;

    for (const selector of agent.inputSelectors) {
      try {
        await page.waitForSelector(selector, { timeout: 5000 });
        messageInput = await page.$(selector);
        if (messageInput) {
          logger.info(`Found input with selector: ${selector}`);
          break;
        }
      } catch (e) {
        logger.debug(`Selector ${selector} not found, trying next...`);
        continue;
      }
    }

    if (!messageInput) {
      logger.error('Could not find message input. Taking screenshot for debugging...');
      await page.screenshot({ path: 'debug.png' });
      throw new Error('Message input not found');
    }

    // Type the message
    logger.info(`Typing message: "${message}"`);
    await messageInput.click();
    await messageInput.type(message);

    // Send the message
    logger.info('Sending message...');
    
    // Try Enter key first (like your working code)
    await page.keyboard.press('Enter');

    // Wait and try button click as backup
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    for (const selector of agent.sendButtonSelectors) {
      try {
        const sendButton = await page.$(selector);
        if (sendButton) {
          logger.debug(`Found send button with selector: ${selector}`);
          await sendButton.click();
          break;
        }
      } catch (e) {
        logger.debug(`Send button selector ${selector} failed`);
        continue;
      }
    }

    // Wait for complete response
    logger.info('Waiting for response...');
    const response = await streamResponse(page, agent);

    // Log the complete response to console
    if (response) {
      console.log('\n' + '='.repeat(80));
      console.log('CLAUDE RESPONSE:');
      console.log('='.repeat(80));
      console.log(response);
      console.log('='.repeat(80));
      console.log(`Response length: ${response.length} characters`);
      console.log('='.repeat(80));
    } else {
      console.log('\n' + '='.repeat(80));
      console.log('NO RESPONSE RECEIVED');
      console.log('='.repeat(80));
    }

    logger.info(`Successfully sent message to ${agent.name}`);
    logger.info('Browser tab is left open for follow-up questions.');
    logger.info('This process will stay alive to keep the browser open.');
    logger.info('The process will end automatically if you close the browser tab or when the task is deleted from vibe-kanban.');

    // Keep the process alive to maintain browser session
    await keepProcessAlive(browser, page);

  } catch (error) {
    logger.error('Error:', error);
    // Close browser on error to avoid orphaned processes
    if (browser) {
      logger.info('Closing browser due to error...');
      await browser.close();
    }
    throw error;
  } finally {
    // Browser will be closed by keepProcessAlive when needed
  }
}

async function keepProcessAlive(browser: Browser, page: Page): Promise<void> {
  logger.info('Process is now running in keep-alive mode...');
  logger.info('Monitoring browser for tab/window close events...');
  
  // Keep process alive indefinitely until killed by parent, system signals, or browser closed
  return new Promise((resolve) => {
    let isShuttingDown = false;
    
    const gracefulShutdown = async (reason: string) => {
      if (isShuttingDown) return;
      isShuttingDown = true;
      
      logger.info(`${reason}, shutting down gracefully...`);
      
      // Clean up browser if still open
      if (browser) {
        try {
          await browser.close();
        } catch (err) {
          logger.debug('Error closing browser during shutdown:', err);
        }
      }
      
      resolve();
    };

    // Handle termination signals gracefully
    process.on('SIGTERM', () => gracefulShutdown('Received SIGTERM'));
    process.on('SIGINT', () => gracefulShutdown('Received SIGINT (Ctrl+C)'));

    // 🔑 NEW: Monitor browser disconnection events
    browser.on('disconnected', () => {
      logger.info('🔍 Browser process disconnected (browser closed or crashed)');
      gracefulShutdown('Browser disconnected');
    });

    // 🔑 NEW: Monitor page close events  
    page.on('close', () => {
      logger.info('🗑️  Browser tab closed by user');
      gracefulShutdown('Browser tab closed');
    });

    // 🔑 NEW: Monitor for page errors that might indicate tab issues
    page.on('error', (error) => {
      logger.info('❌ Page error detected:', error.message);
      gracefulShutdown('Page error detected');
    });

    // 🔑 NEW: Monitor for page crashes
    page.on('pageerror', (error) => {
      logger.info('💥 Page crash detected:', error.message);
      gracefulShutdown('Page crashed');
    });

    // 🔑 NEW: Monitor for browser process termination
    const checkBrowserAlive = setInterval(async () => {
      try {
        // Try to get browser version - this will fail if browser is closed
        await browser.version();
        
        // Try to get page title - this will fail if page is closed
        await page.title();
      } catch (error) {
        logger.info('🔍 Browser health check failed (likely closed)');
        gracefulShutdown('Browser health check failed');
      }
    }, 10000); // Check every 10 seconds

    // Log keep-alive status every 5 minutes
    const keepAliveInterval = setInterval(() => {
      if (!isShuttingDown) {
        logger.debug('Browser automation process still alive, keeping browser session open...');
      }
    }, 5 * 60 * 1000); // 5 minutes

    // Clean up intervals when process ends
    const cleanup = () => {
      clearInterval(keepAliveInterval);
      clearInterval(checkBrowserAlive);
    };
    
    process.on('exit', cleanup);
    
    // Cleanup when promise resolves
    setTimeout(() => {
      if (isShuttingDown) {
        cleanup();
      }
    }, 1000);
  });
}

async function streamResponse(page: Page, _agent: AgentConfig): Promise<string> {
  // Enhanced response selectors for Claude's interface
  const responseSelectors = [
    '[data-testid="message-content"]:last-child',
    '.message:last-child .content',
    '[role="article"]:last-child',
    '.prose:last-child',
    '[data-message-author="assistant"]:last-child',
    'div[data-is-streaming="false"]:last-child',
    'div.group:last-child [data-testid="message-content"]'
  ];

  // Claude-specific selectors to detect when response is complete
  const completionIndicators = [
    // Copy button appears when response is complete
    'button[aria-label="Copy"]',
    'button[aria-label*="copy"]',
    '[data-testid="copy-button"]',
    // Response container is no longer in streaming state
    '[data-is-streaming="false"]',
    // Typing indicator disappears
    '.typing-indicator[style*="display: none"]'
  ];

  let finalResponse = '';
  let attempts = 0;
  const maxAttempts = 240; // 2 minutes max wait time
  let lastResponseLength = 0;
  let stableResponseCount = 0;

  logger.info('Waiting for Claude to start responding...');

  while (attempts < maxAttempts) {
    try {
      let currentResponse = '';

      // Try to get the response content
      for (const selector of responseSelectors) {
        try {
          const element = await page.$(selector);
          if (element) {
            currentResponse = await page.evaluate((el: Element) => {
              // Get text content, preserving line breaks
              return el.textContent || (el as HTMLElement).innerText || '';
            }, element);
            
            if (currentResponse && currentResponse.trim()) {
              break;
            }
          }
        } catch (e) {
          continue;
        }
      }

      // If we have content, check if it's stable (not changing)
      if (currentResponse && currentResponse.trim()) {
        const currentLength = currentResponse.length;
        
        // Response length hasn't changed - it might be complete
        if (currentLength === lastResponseLength) {
          stableResponseCount++;
        } else {
          stableResponseCount = 0;
          lastResponseLength = currentLength;
        }

        finalResponse = currentResponse;

        // Check for completion indicators
        let isComplete = false;
        
        // Method 1: Check for completion UI indicators
        for (const indicator of completionIndicators) {
          try {
            const element = await page.$(indicator);
            if (element) {
              // Additional check - make sure the copy button is visible and clickable
              const isVisible = await page.evaluate((el: Element) => {
                const rect = el.getBoundingClientRect();
                return rect.width > 0 && rect.height > 0;
              }, element);
              
              if (isVisible) {
                logger.debug(`Found completion indicator: ${indicator}`);
                isComplete = true;
                break;
              }
            }
          } catch (e) {
            continue;
          }
        }

        // Method 2: Response has been stable for several checks
        if (stableResponseCount >= 6) { // 3 seconds of stability
          logger.debug('Response appears stable, assuming complete');
          isComplete = true;
        }

        // Method 3: Check if typing indicator is gone
        try {
          const typingIndicator = await page.$('.typing-indicator, [data-testid="typing-indicator"]');
          if (!typingIndicator) {
            // No typing indicator found, response might be complete
            if (stableResponseCount >= 2) { // 1 second of stability without typing indicator
              isComplete = true;
            }
          }
        } catch (e) {
          // Ignore errors checking for typing indicator
        }

        if (isComplete) {
          // Wait a bit more to ensure truly complete
          await new Promise(resolve => setTimeout(resolve, 1000));
          logger.info('Response appears complete');
          break;
        }
      }

      await new Promise(resolve => setTimeout(resolve, 500));
      attempts++;

      // Log progress every 10 seconds
      if (attempts % 20 === 0) {
        logger.info(`Still waiting for response... (${Math.round(attempts * 0.5)}s elapsed)`);
      }

    } catch (error) {
      logger.debug('Error in response detection:', error);
      attempts++;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
  }

  if (attempts >= maxAttempts) {
    logger.info('Response detection timeout - returning whatever content was found');
  }

  return finalResponse;
}

// CLI Interface
async function main(): Promise<void> {
  const args = process.argv.slice(2);
  
  if (args.length < 2) {
    console.error('Usage: npm run dev <agent> "<message>"');
    console.error('Agents: claude, m365');
    console.error('Example: npm run dev claude "Hello, can you help me?"');
    process.exit(1);
  }

  const [agentName, ...messageArgs] = args;
  const message = messageArgs.join(' ');

  // Handle process termination gracefully
  process.on('SIGINT', () => {
    logger.info('Received SIGINT, exiting...');
    process.exit(0);
  });

  process.on('SIGTERM', () => {
    logger.info('Received SIGTERM, exiting...');
    process.exit(0);
  });

  try {
    await sendMessageToAgent(agentName, message);
  } catch (error) {
    logger.error('Fatal error:', error);
    process.exit(1);
  }
}

// Only run main if this file is executed directly
if (require.main === module) {
  main().catch(error => {
    console.error('Fatal error:', error);
    process.exit(1);
  });
}

// Handle follow-up messages (reuse existing browser session)
async function sendFollowUpMessage(agent: AgentConfig, message: string, sessionId: string): Promise<string> {
  // For now, follow-up messages create a new session
  // TODO: Implement true session reuse by connecting to existing browser instance
  logger.info(`Follow-up message for session ${sessionId} - creating new browser instance for now`);
  return await sendInitialMessage(agent, message);
}

// Function specifically for getting Claude's complete response
async function sendMessageAndGetResponse(agentName: string, message: string, sessionId?: string): Promise<string> {
  const agent = agents[agentName.toLowerCase()];
  if (!agent) {
    throw new Error(`Unknown agent: ${agentName}. Available: ${Object.keys(agents).join(', ')}`);
  }

  if (sessionId) {
    logger.info(`Using session ID: ${sessionId} for follow-up message`);
    return await sendFollowUpMessage(agent, message, sessionId);
  } else {
    logger.info('Starting new browser chat session');
    return await sendInitialMessage(agent, message);
  }
}

// Initial message (creates new browser session)
async function sendInitialMessage(agent: AgentConfig, message: string): Promise<string> {
  let browser: Browser | null = null;

  try {
    logger.info(`Initializing ${agent.name} automation to get response...`);
    
    // Create persistent user data directory for this agent
    const userDataDir = path.join(os.homedir(), '.browser-automation', `${agent.name.toLowerCase().replace(/\s+/g, '-')}-profile`);
    
    browser = await puppeteer.launch({
      headless: false, // Always visible for manual login
      userDataDir, // Enable session persistence
      args: [
        '--no-sandbox',
        '--disable-setuid-sandbox',
        '--disable-dev-shm-usage',
        '--disable-accelerated-2d-canvas',
        '--no-first-run',
        '--no-zygote',
        '--disable-gpu'
      ]
    });

    const page: Page = await browser.newPage();

    // Set viewport and user agent
    await page.setViewport({ width: 1366, height: 768 });
    await page.setUserAgent(
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36'
    );

    // Navigate to the agent
    logger.info(`Navigating to ${agent.name} at ${agent.url}...`);
    await page.goto(agent.url, {
      waitUntil: 'networkidle2',
      timeout: 120000 // 2 minutes
    });

    // Wait for page to load
    await new Promise(resolve => setTimeout(resolve, 5000));

    // Check if already logged in
    const isLoggedIn = await checkIfLoggedIn(page, agent);
    
    if (!isLoggedIn) {
      // Automatic login detection - wait for user to complete login in browser
      logger.info('🔐 Login required: Please complete authentication in the browser window.');
      logger.info('⏳ Waiting for login to complete automatically...');
      logger.info('   (No manual input needed - login will be detected automatically)');
      
      await waitForLogin(page, agent);
      logger.info('✅ Login detected! Continuing with automation...');
    }

    // Find message input
    let messageInput = null;
    for (const selector of agent.inputSelectors) {
      try {
        await page.waitForSelector(selector, { timeout: 5000 });
        messageInput = await page.$(selector);
        if (messageInput) {
          logger.info(`Found input with selector: ${selector}`);
          break;
        }
      } catch (e) {
        continue;
      }
    }

    if (!messageInput) {
      throw new Error('Message input not found');
    }

    // Send the message
    await messageInput.click();
    await messageInput.type(message);
    await page.keyboard.press('Enter');

    // Wait a moment then try button click as backup
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    for (const selector of agent.sendButtonSelectors) {
      try {
        const sendButton = await page.$(selector);
        if (sendButton) {
          await sendButton.click();
          break;
        }
      } catch (e) {
        continue;
      }
    }

    // Get the complete response
    logger.info('Waiting for complete response...');
    const response = await streamResponse(page, agent);

    return response;

  } catch (error) {
    logger.error('Error in sendMessageAndGetResponse:', error);
    throw error;
  } finally {
    if (browser) {
      logger.info('Closing browser...');
      await browser.close();
    }
  }
}

export { sendMessageToAgent, sendMessageAndGetResponse };